use actix_msgpack::MsgPackResponseBuilder;
use actix_web::{get, HttpResponse, Responder};
use log::trace;

use crate::{project::ProjectDetails, server::CoreRef};

#[get("/details")]
async fn main(core: CoreRef) -> impl Responder {
	trace!("Received request: details");
	HttpResponse::Ok().msgpack(ProjectDetails::from_project(&core.project(), &core.tree()))
}
