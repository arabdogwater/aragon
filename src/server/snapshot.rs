use actix_msgpack::{MsgPack, MsgPackResponseBuilder};
use actix_web::{post, HttpResponse, Responder};
use log::trace;
use rbx_dom_weak::types::Ref;
use serde::Deserialize;

use crate::server::CoreRef;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Request {
	instance: Ref,
}

#[post("/snapshot")]
async fn main(request: MsgPack<Request>, core: CoreRef) -> impl Responder {
	trace!("Received request: snapshot");
	HttpResponse::Ok().msgpack(core.snapshot(request.instance))
}
