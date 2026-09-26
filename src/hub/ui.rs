//! Serves the dashboard web app. The files are embedded in the binary; set
//! `ARAGON_DASHBOARD_DIR` to serve them from disk while developing the UI.

use actix_web::{
	web::{self, ServiceConfig},
	HttpRequest, HttpResponse,
};
use include_dir::{include_dir, Dir};
use std::{env, fs, path::PathBuf};

static DASHBOARD: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/dashboard");

fn content_type(path: &str) -> &'static str {
	match path.rsplit('.').next().unwrap_or_default() {
		"html" => "text/html; charset=utf-8",
		"css" => "text/css; charset=utf-8",
		"js" | "mjs" => "text/javascript; charset=utf-8",
		"json" => "application/json",
		"svg" => "image/svg+xml",
		"png" => "image/png",
		"webp" => "image/webp",
		"jpg" | "jpeg" => "image/jpeg",
		"ico" => "image/x-icon",
		"woff2" => "font/woff2",
		_ => "application/octet-stream",
	}
}

async fn serve(request: HttpRequest) -> HttpResponse {
	let tail = request.match_info().query("tail").trim_start_matches('/');
	let path = if tail.is_empty() { "index.html" } else { tail };

	if path.contains("..") {
		return HttpResponse::NotFound().finish();
	}

	let bytes = match env::var_os("ARAGON_DASHBOARD_DIR") {
		Some(dir) => fs::read(PathBuf::from(dir).join(path)).ok(),
		None => DASHBOARD.get_file(path).map(|file| file.contents().to_vec()),
	};

	match bytes {
		Some(bytes) => HttpResponse::Ok()
			.content_type(content_type(path))
			.insert_header(("Cache-Control", "no-cache"))
			.body(bytes),
		None => HttpResponse::NotFound().finish(),
	}
}

pub fn configure(config: &mut ServiceConfig) {
	config
		.route("/ui/{tail:.*}", web::get().to(serve))
		.route("/ui", web::get().to(|| async { web::Redirect::to("/ui/") }))
		.route("/", web::get().to(|| async { web::Redirect::to("/ui/") }));
}
