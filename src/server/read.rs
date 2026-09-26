use actix_msgpack::{MsgPack, MsgPackResponseBuilder};
use actix_web::{post, web, HttpResponse, Responder};
use log::trace;

use crate::server::{AuthRequest, CoreRef};

#[post("/read")]
async fn main(request: MsgPack<AuthRequest>, core: CoreRef) -> impl Responder {
	trace!("Received request: read");

	let id = request.client_id;
	let queue = core.queue();

	if !queue.is_subscribed(id) {
		return HttpResponse::Unauthorized().body("Not subscribed");
	}

	// Long-poll on the blocking pool so many Studio sessions can
	// wait at once without starving the actix workers
	match web::block(move || queue.get_timeout(id)).await {
		Ok(Ok(message)) => HttpResponse::Ok().msgpack(message),
		Ok(Err(err)) => HttpResponse::InternalServerError().body(err.to_string()),
		Err(err) => HttpResponse::InternalServerError().body(err.to_string()),
	}
}
