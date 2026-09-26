use actix_msgpack::MsgPack;
use actix_web::{post, HttpResponse, Responder};
use log::trace;
use serde::Deserialize;

use crate::server::CoreRef;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Request {
	client_id: u32,
	name: String,
}

#[post("/subscribe")]
async fn main(request: MsgPack<Request>, core: CoreRef) -> impl Responder {
	trace!("Received request: subscribe");

	let subscribed = core.queue().subscribe(request.client_id, &request.name);

	if subscribed.is_ok() {
		HttpResponse::Ok().body("Subscribed successfully")
	} else {
		HttpResponse::BadRequest().body("Already subscribed")
	}
}
