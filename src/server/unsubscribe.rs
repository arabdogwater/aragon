use actix_msgpack::MsgPack;
use actix_web::{post, HttpResponse, Responder};
use log::trace;

use crate::server::{AuthRequest, CoreRef};

#[post("/unsubscribe")]
async fn main(request: MsgPack<AuthRequest>, core: CoreRef) -> impl Responder {
	trace!("Received request: unsubscribe");

	let unsubscribed = core.queue().unsubscribe(request.client_id);

	if unsubscribed.is_ok() {
		HttpResponse::Ok().body("Unsubscribed successfully")
	} else {
		HttpResponse::BadRequest().body("Not subscribed")
	}
}
