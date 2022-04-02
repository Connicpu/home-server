use chrono::{DateTime, Duration, Utc};
use digest::{Digest, KeyInit};
use hmac::Hmac;
use jwt::{Header, SignWithKey, Token, Verified, VerifyWithKey};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Sha384};
use warp::{
    filters::{header::header, path, BoxedFilter},
    reject::Reject,
    Filter, Rejection, Reply,
};

use crate::{error::WebErrorExt, RedisConn};

const TOKEN_DAYS: i64 = 30;

#[derive(Copy, Clone, Debug, Serialize)]
pub enum AuthFailed {
    Credentials,
    InvalidToken,
    Expired,
    Permission,
    WeakPassword,
    NotApproved,
    AccountExists,
}
impl Reject for AuthFailed {}
#[derive(Debug)]
struct RedisError(redis::RedisError);
impl Reject for RedisError {}

pub fn with_auth(level: i32) -> BoxedFilter<()> {
    warp::filters::header::header("X-Auth")
        .and_then(move |auth: String| validate_auth_token(auth, level))
        .untuple_one()
        .boxed()
}

pub async fn routes(redis: RedisConn) -> BoxedFilter<(impl Reply,)> {
    let login = {
        let redis = redis.clone();
        warp::path("login")
            .and(path::end())
            .and(header("X-Username"))
            .and(header("X-Password"))
            .and_then(move |user: String, pass: String| {
                let redis = redis.clone();
                async move {
                    if validate_credentials(&redis, &user, &pass).await? {
                        Ok(generate_auth_token(&redis, &user).await?)
                    } else {
                        Err(warp::reject::custom(AuthFailed::Credentials))
                    }
                }
            })
    };

    let renew = {
        let redis = redis.clone();
        warp::path("renew")
            .and(path::end())
            .and(header("X-Auth"))
            .and_then(move |token: String| renew_auth_token(redis.clone(), token))
    };

    let register = {
        let redis = redis.clone();
        warp::path("register")
            .and(path::end())
            .and(warp::put())
            .and(header("X-Username"))
            .and(header("X-Password"))
            .and_then(move |user: String, pass: String| {
                register(redis.clone(), user, pass)
            })
    };

    let put_password = {
        let redis = redis.clone();
        warp::path("password")
            .and(path::end())
            .and(warp::put())
            .and(header("X-Auth"))
            .and(header("X-Password"))
            .and_then(move |auth: String, new_password: String| {
                change_password(redis.clone(), auth, new_password)
            })
    };

    let put_auth_level = {
        let redis = redis.clone();
        warp::path("auth_level")
            .and(path::end())
            .and(warp::put())
            .and(header("X-Username"))
            .and(header("X-AuthLevel"))
            .and(with_auth(3))
            .and_then(move |user: String, level: i32| {
                set_auth_level(redis.clone(), user, level)
            })
    };

    login.or(renew).or(register).or(put_password).or(put_auth_level).boxed()
}

async fn validate_credentials(
    redis: &RedisConn,
    user: &str,
    pass: &str,
) -> Result<bool, Rejection> {
    let hash = hex::encode(Sha256::digest(pass));

    let saved_hash: String = {
        let mut redis = redis.get();
        redis.hget("auth.password", user).await.reject_err()?
    };

    Ok(hash.eq_ignore_ascii_case(&saved_hash))
}

async fn generate_auth_token(redis: &RedisConn, user: &str) -> Result<String, Rejection> {
    let auth_level: i32 = {
        let mut redis = redis.get();
        redis.hget("auth.level", user).await.reject_err()?
    };

    let claims = Authentication {
        user: user.into(),
        valid_until: Utc::now() + Duration::days(TOKEN_DAYS),
        auth_level,
    };

    let header = Header {
        algorithm: jwt::AlgorithmType::Hs384,
        ..Default::default()
    };

    Ok(Token::new(header, claims)
        .sign_with_key(&jwt_key())
        .reject_err()?
        .as_str()
        .to_string())
}

fn verify_auth_token(token: String) -> Result<Authentication, Rejection> {
    type VerifiedToken = Token<Header, Authentication, Verified>;
    let token: VerifiedToken = token.verify_with_key(&jwt_key()).map_err(|_| AuthFailed::InvalidToken)?;
    Ok(token.claims().clone())
}

async fn validate_auth_token(token: String, level: i32) -> Result<(), Rejection> {
    let claims = verify_auth_token(token)?;
    if claims.valid_until < Utc::now() {
        return Err(AuthFailed::Expired.into());
    }
    if claims.auth_level < level {
        return Err(AuthFailed::Permission.into());
    }
    Ok(())
}

async fn renew_auth_token(redis: RedisConn, token: String) -> Result<String, Rejection> {
    let mut claims = verify_auth_token(token)?;

    let mut redis = redis.get();
    claims.auth_level = redis.hget("auth.level", &claims.user).await.reject_err()?;
    claims.valid_until = Utc::now() + Duration::days(TOKEN_DAYS);
    let token = claims.sign_with_key(&jwt_key()).reject_err()?;

    Ok(token)
}

async fn change_password(
    redis: RedisConn,
    token: String,
    new_password: String,
) -> Result<String, Rejection> {
    if new_password.len() < 12 {
        return Err(AuthFailed::WeakPassword.into());
    }

    let auth = verify_auth_token(token)?;
    let hash = hex::encode(Sha256::digest(new_password));

    let mut redis = redis.get();
    let () = redis
        .hset("auth.password", &auth.user, hash)
        .await
        .reject_err()?;

    Ok("ok".into())
}

async fn set_auth_level(redis: RedisConn, user: String, level: i32) -> Result<String, Rejection> {
    let mut redis = redis.get();
    let () = redis.hset("auth.level", &user, level).await.reject_err()?;
    Ok("ok".into())
}

async fn register(redis: RedisConn, user: String, pass: String) -> Result<String, Rejection> {
    let hash = hex::encode(Sha256::digest(pass));

    {
        let mut redis = redis.get();
        let Some(_): Option<i32> = redis.hget("auth.level", &user).await.reject_err()? else {
            return Err(AuthFailed::NotApproved.into());
        };
        let None: Option<String> = redis.hget("auth.password", &user).await.reject_err()? else {
            return Err(AuthFailed::AccountExists.into());
        };
    
        let () = redis.hset("auth.password", &user, hash).await.reject_err()?;
    }

    generate_auth_token(&redis, &user).await
}

fn jwt_key() -> Hmac<Sha384> {
    const SECRET: &str = dotenv_codegen::dotenv!("JWT_SECRET");
    KeyInit::new_from_slice(SECRET.as_bytes()).unwrap()
}

#[derive(Clone, Serialize, Deserialize)]
struct Authentication {
    user: String,
    valid_until: DateTime<Utc>,
    auth_level: i32,
}
