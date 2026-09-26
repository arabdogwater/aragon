use actix_msgpack::MsgPack;
use actix_web::{post, HttpResponse, Responder};
use log::trace;

use crate::{core::processor::WriteRequest, server::CoreRef};

#[post("/write")]
async fn main(request: MsgPack<WriteRequest>, core: CoreRef) -> impl Responder {
	trace!("Received request: write");

	let request = request.0;

	if !core.queue().is_subscribed(request.client_id) {
		return HttpResponse::Unauthorized().body("Not subscribed");
	}

	core.processor().write(request);

	HttpResponse::Ok().body("Written changes successfully")
}
