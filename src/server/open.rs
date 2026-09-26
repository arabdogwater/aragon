use actix_msgpack::MsgPack;
use actix_web::{post, HttpResponse, Responder};
use log::trace;
use rbx_dom_weak::types::Ref;
use serde::Deserialize;

use crate::server::CoreRef;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Request {
	instance: Ref,
	_line: u32,
}

#[post("/open")]
async fn main(request: MsgPack<Request>, core: CoreRef) -> impl Responder {
	trace!("Received request: open");

	match core.open(request.instance) {
		Ok(_) => HttpResponse::Ok().body("Opened file successfully"),
		Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
	}
}
