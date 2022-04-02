use std::collections::HashMap;

pub struct Handler(Box<dyn Fn(&str, &[u8]) + Send + Sync + 'static>);

impl<F> From<F> for Handler
where
    F: Fn(&str, &[u8]) + Send + Sync + 'static,
{
    fn from(handler: F) -> Handler {
        Handler(Box::new(handler))
    }
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        ((&self.0) as *const _ as *const ()).fmt(fmt)
    }
}

pub struct Router {
    root: Route,
}

impl Router {
    pub fn new() -> Self {
        Router {
            root: Route::Leaf(vec![]),
        }
    }

    pub fn insert(&mut self, path: &str, handler: Handler) {
        insert(&mut self.root, path, handler);
    }

    pub fn dispatch(&self, topic: &str, payload: &[u8]) {
        dispatch(&self.root, topic, topic, payload);
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum Route {
    Leaf(Vec<Handler>),
    Node(Box<RouteNode>),
}

#[derive(Debug)]
struct RouteNode {
    handlers: Vec<Handler>,
    children: HashMap<String, Route>,
}

fn make_node(route: &mut Route) -> &mut RouteNode {
    match route {
        Route::Leaf(handlers) => {
            *route = Route::Node(Box::new(RouteNode {
                handlers: std::mem::take(handlers),
                children: Default::default(),
            }));
            make_node(route)
        }
        Route::Node(node) => node,
    }
}

fn handlers_of(route: &Route) -> &Vec<Handler> {
    match route {
        Route::Leaf(handlers) => handlers,
        Route::Node(node) => &node.handlers,
    }
}

fn handlers_of_mut(route: &mut Route) -> &mut Vec<Handler> {
    match route {
        Route::Leaf(handlers) => handlers,
        Route::Node(node) => &mut node.handlers,
    }
}

fn insert(route: &mut Route, path: &str, handler: Handler) {
    if path.is_empty() {
        handlers_of_mut(route).push(handler);
        return;
    }

    let (stem, leaf) = if let Some((stem, leaf)) = path.split_once('/') {
        (stem, leaf)
    } else {
        (path, "")
    };

    let inner = make_node(route)
        .children
        .entry(stem.into())
        .or_insert_with(|| Route::Leaf(vec![]));
    insert(inner, leaf, handler);
}

fn dispatch(route: &Route, path: &str, topic: &str, payload: &[u8]) {
    if path.is_empty() {
        execute(route, topic, payload);
        return;
    }

    if let Some(route) = sub_tree(route, "*") {
        execute(route, topic, payload)
    }

    let (stem, leaf) = if let Some((stem, leaf)) = path.split_once('/') {
        (stem, leaf)
    } else {
        (path, "")
    };

    if let Some(route) = sub_tree(route, stem) {
        dispatch(route, leaf, topic, payload);
    }
}

fn sub_tree<'a>(route: &'a Route, stem: &str) -> Option<&'a Route> {
    match route {
        Route::Node(node) => node.children.get(stem),
        _ => None,
    }
}

fn execute(route: &Route, topic: &str, payload: &[u8]) {
    for handler in handlers_of(route) {
        (handler.0)(topic, payload);
    }
}
