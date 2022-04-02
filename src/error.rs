use warp::{Rejection, reject::Reject};

pub trait WebErrorExt {
    type Out;
    fn reject_err(self) -> Self::Out;
}

impl<T, E: Into<anyhow::Error>> WebErrorExt for Result<T, E> {
    type Out = Result<T, Rejection>;
    fn reject_err(self) -> Self::Out {
        self.map_err(|e| warp::reject::custom(ServerError(e.into())))
    }
}

#[derive(Debug)]
struct ServerError(anyhow::Error);

impl Reject for ServerError {}

