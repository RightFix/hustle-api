use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};

use crate::AppState;

pub mod admin;
pub mod auth;
pub mod avatar;
pub mod blocks;
pub mod gallery;
pub mod gigs;
pub mod health;
pub mod messages;
pub mod notifications;
pub mod posts;
pub mod reports;
pub mod users;

pub fn router(upload_dir: &str) -> Router<AppState> {
    let api = Router::new()
        .route("/auth/register", axum::routing::post(auth::register).route_layer(middleware::from_fn(crate::rate_limit::auth_strict)))
        .route("/auth/login", axum::routing::post(auth::login).route_layer(middleware::from_fn(crate::rate_limit::auth_strict)))
        .route("/auth/logout", axum::routing::post(auth::logout))
        .route(
            "/auth/forgot-password",
            axum::routing::post(auth::forgot_password).route_layer(middleware::from_fn(crate::rate_limit::auth_strict)),
        )
        .route("/auth/reset-password", axum::routing::post(auth::reset_password).route_layer(middleware::from_fn(crate::rate_limit::auth_strict)))
        .route("/auth/verify-email", get(auth::verify_email))
        .route(
            "/auth/resend-verification",
            axum::routing::post(auth::resend_verification),
        )
        .route("/users", get(users::search_users))
        .route("/users/me", get(users::me).patch(users::update_me))
        .route("/users/me/skills", put(users::update_skills))
        .route("/users/{user_id}", get(users::read_user))
        .route("/users/{user_id}/skills", get(users::read_skills))
        .route(
            "/users/{user_id}/follow",
            axum::routing::post(users::follow_user)
                .route_layer(middleware::from_fn(crate::rate_limit::interact_limit))
                .delete(users::unfollow_user),
        )
        .route(
            "/users/{user_id}/block",
            axum::routing::post(blocks::block_user).delete(blocks::unblock_user),
        )
        .route("/users/{user_id}/follow-info", get(users::follow_info))
        .route("/users/me/blocks", get(blocks::list_my_blocks))
        .route("/posts", axum::routing::post(posts::create_post).route_layer(middleware::from_fn(crate::rate_limit::write_actions)))
        .route("/posts/feed", get(posts::read_feed))
        .route(
            "/posts/{post_id}",
            get(posts::read_post).delete(posts::delete_post),
        )
        .route("/posts/user/{user_id}", get(posts::read_user_posts));

    let admin = Router::new()
        .route("/admin/stats", get(admin::stats))
        .route("/admin/users", get(admin::list_users))
        .route(
            "/admin/users/{id}",
            get(admin::get_user)
                .patch(admin::update_user)
                .delete(admin::delete_user),
        )
        .route("/admin/posts", get(admin::list_posts))
        .route("/admin/posts/{id}", delete(admin::delete_post))
        .route("/admin/gigs", get(admin::list_gigs))
        .route("/admin/gigs/{id}", delete(admin::delete_gig))
        .route("/admin/reports", get(reports::admin_list_reports))
        .route("/admin/reports/{id}", axum::routing::patch(reports::admin_resolve_report))
        .route("/admin/audit", get(admin::list_audit));

    let gigs = Router::new()
        .route("/gigs", get(gigs::list_gigs).post(gigs::create_gig))
        .route("/gigs/saved", get(gigs::saved_gigs))
        .route(
            "/gigs/{gig_id}/save",
            post(gigs::save_gig).delete(gigs::unsave_gig),
        )
        .route(
            "/gigs/{gig_id}",
            get(gigs::read_gig)
                .patch(gigs::update_gig)
                .delete(gigs::delete_gig),
        )
        .route(
            "/gigs/{gig_id}/apply",
            post(gigs::apply_gig).delete(gigs::withdraw_gig),
        )
        .route("/gigs/{gig_id}/applicants", get(gigs::list_applicants));

    let conversations = Router::new()
        .route(
            "/conversations",
            get(messages::list_conversations).post(messages::start_conversation),
        )
        .route(
            "/conversations/{conversation_id}/messages",
            get(messages::list_messages),
        )
        .route(
            "/conversations/{conversation_id}/read",
            post(messages::mark_read),
        );

    let notifications = Router::new()
        .route("/notifications", get(notifications::list_notifications))
        .route("/notifications/unread-count", get(notifications::unread_count))
        .route(
            "/notifications/{notification_id}/read",
            post(notifications::mark_read),
        )
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .route("/reports", post(reports::create_report).route_layer(middleware::from_fn(crate::rate_limit::report_limit)));

    let gallery = Router::new()
        .route(
            "/users/me/gallery",
            post(gallery::upload_gallery_item),
        )
        .route(
            "/users/{user_id}/gallery",
            get(gallery::list_gallery),
        )
        .route(
            "/gallery/{item_id}",
            axum::routing::delete(gallery::delete_gallery_item),
        )
        // uploads allow up to 50 MB media; axum's default body limit is 2 MB
        .layer(axum::extract::DefaultBodyLimit::max(52 * 1024 * 1024));

    let avatar = Router::new()
        .route(
            "/users/me/avatar",
            post(avatar::upload_avatar).delete(avatar::delete_avatar),
        )
        // match the image upload ceiling plus multipart overhead
        .layer(axum::extract::DefaultBodyLimit::max(11 * 1024 * 1024));

    Router::new()
        .route("/health", get(health::health))
        .nest_service(
            "/media",
            tower_http::services::ServeDir::new(upload_dir),
        )
        .nest(
            "/api",
            api.merge(admin)
                .merge(gigs)
                .merge(conversations)
                .merge(notifications)
                .merge(gallery)
                .merge(avatar),
        )
        .route("/api/ws", get(crate::ws::ws_handler))
}
