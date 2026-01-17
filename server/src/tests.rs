#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    // helper to construct app with a given pool
    async fn make_app(pool: SqlitePool) -> actix_web::App<impl actix_web::dev::HttpServiceFactory> {
        App::new()
            .app_data(actix_web::web::Data::new(pool))
            .service(
                actix_web::web::scope("/api")
                    .route("/register", actix_web::web::post().to(crate::handlers::register_user))
                    .route("/login", actix_web::web::post().to(crate::handlers::login_user))
                    .route("/scores", actix_web::web::post().to(crate::handlers::post_score))
                    .route("/leaderboard/{song_id}", actix_web::web::get().to(crate::handlers::get_leaderboard))
                    .route("/songs", actix_web::web::get().to(crate::handlers::list_songs))
                    .route("/songs/{id}/stream", actix_web::web::get().to(crate::handlers::stream_song))
            )
    }

    #[actix_rt::test]
    async fn register_login_and_score_flow() {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("connect");
        crate::db::init_db(&pool).await.expect("init db");

        let app = test::init_service(make_app(pool.clone()).await).await;

        // register
        let reg = serde_json::json!({"username":"alice","password":"pass123"});
        let req = test::TestRequest::post().uri("/api/register").set_json(&reg).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

        // login
        let req = test::TestRequest::post().uri("/api/login").set_json(&reg).to_request();
        let resp = test::call_and_read_body_json::<_, serde_json::Value>(&app, req).await;
        let token = resp.get("token").expect("token").as_str().expect("str");

        // post online score
        let score = serde_json::json!({"song_id":"song1","player":"ignored","score":9000,"online":true});
        let req = test::TestRequest::post().uri("/api/scores").insert_header(("Authorization", format!("Bearer {}", token))).set_json(&score).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        // get leaderboard
        let req = test::TestRequest::get().uri("/api/leaderboard/song1").to_request();
        let body = test::call_and_read_body_json::<_, serde_json::Value>(&app, req).await;
        assert!(body.as_array().unwrap().len() >= 1);
    }

    #[actix_rt::test]
    async fn list_and_stream_songs() {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("connect");
        crate::db::init_db(&pool).await.expect("init db");

        // create temp songs dir
        let tmp = TempDir::new().expect("tempdir");
        let song_path = tmp.path().join("foo.wav");
        std::fs::write(&song_path, b"RIFF....").expect("write");
        std::env::set_var("SONGS_DIR", tmp.path().to_str().expect("str"));

        let app = test::init_service(make_app(pool.clone()).await).await;

        let req = test::TestRequest::get().uri("/api/songs").to_request();
        let list = test::call_and_read_body_json::<_, serde_json::Value>(&app, req).await;
        assert!(list.as_array().unwrap().iter().any(|v| v.get("filename").unwrap().as_str().unwrap().contains("foo.wav")));

        let req = test::TestRequest::get().uri("/api/songs/foo/stream").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }
}
