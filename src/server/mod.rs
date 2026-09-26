use actix_msgpack::MsgPackConfig;
use actix_web::{
	dev::Payload,
	error::{ErrorInternalServerError, ErrorNotFound},
	web::{self, Data, ServiceConfig},
	App, FromRequest, HttpRequest, HttpServer, Responder,
};
use derive_from_one::FromOne;
use serde::{Deserialize, Serialize};
use std::{
	future::{ready, Ready},
	io::Result,
	net::TcpListener,
	ops::Deref,
	sync::Arc,
};

use crate::{
	constants::MAX_PAYLOAD_SIZE,
	core::{changes::Changes, Core},
	hub::Hub,
	project::ProjectDetails,
};

mod details;
mod exec;
mod home;
mod open;
mod read;
mod snapshot;
mod stop;
mod subscribe;
mod unsubscribe;
mod write;

#[derive(Debug, Clone, Serialize, FromOne)]
pub enum Message {
	SyncChanges(SyncChanges),
	SyncbackChanges(SyncbackChanges),
	SyncDetails(SyncDetails),
	ExecuteCode(ExecuteCode),
	Disconnect(Disconnect),
}

impl Message {
	pub fn is_change(&self) -> bool {
		matches!(self, Message::SyncChanges(_) | Message::SyncbackChanges(_))
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncChanges(pub Changes);

#[derive(Debug, Clone, Serialize)]
pub struct SyncbackChanges();

#[derive(Debug, Clone, Serialize)]
pub struct SyncDetails(pub ProjectDetails);

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteCode {
	pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Disconnect {
	pub message: String,
}

/// Resolves the `Core` a sync request targets. Inside the hub every place
/// lives under `/p/{place}` and is looked up in the hub registry, while the
/// classic single-project `aragon serve` server just hands out its only core
pub struct CoreRef(pub Arc<Core>);

impl Deref for CoreRef {
	type Target = Core;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl FromRequest for CoreRef {
	type Error = actix_web::Error;
	type Future = Ready<std::result::Result<Self, Self::Error>>;

	fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
		if let Some(place) = request.match_info().get("place") {
			let result = match request.app_data::<Data<Arc<Hub>>>() {
				Some(hub) => hub
					.core_for(place)
					.map(CoreRef)
					.ok_or_else(|| ErrorNotFound("Place is not mapped to any project")),
				None => Err(ErrorInternalServerError("Hub is not running")),
			};

			return ready(result);
		}

		ready(match request.app_data::<Data<Arc<Core>>>() {
			Some(core) => Ok(CoreRef(core.get_ref().clone())),
			None => Err(ErrorInternalServerError("No project is being served")),
		})
	}
}

/// Registers every endpoint the Studio plugin uses to sync a single project
pub fn configure_sync(config: &mut ServiceConfig) {
	config
		.service(details::main)
		.service(subscribe::main)
		.service(unsubscribe::main)
		.service(snapshot::main)
		.service(read::main)
		.service(write::main)
		.service(exec::main)
		.service(open::main);
}

pub fn msgpack_config() -> MsgPackConfig {
	let mut msgpack_config = MsgPackConfig::default();
	msgpack_config.limit(MAX_PAYLOAD_SIZE);
	msgpack_config
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
	pub client_id: u32,
}

pub struct Server {
	core: Arc<Core>,
	host: String,
	port: u16,
}

impl Server {
	pub fn new(core: Arc<Core>, host: &str, port: u16) -> Self {
		Self {
			core,
			host: host.to_owned(),
			port,
		}
	}

	#[actix_web::main]
	pub async fn start(&self) -> Result<()> {
		let core = self.core.clone();

		HttpServer::new(move || {
			App::new()
				.app_data(Data::new(core.clone()))
				.app_data(msgpack_config())
				.configure(configure_sync)
				.service(stop::main)
				.service(home::main)
				.default_service(web::to(Self::default_redirect))
		})
		.backlog(0)
		.disable_signals()
		.bind((self.host.clone(), self.port))?
		.run()
		.await
	}

	async fn default_redirect() -> impl Responder {
		web::Redirect::to("/")
	}
}

pub fn is_port_free(host: &str, port: u16) -> bool {
	TcpListener::bind((host, port)).is_ok()
}

pub fn get_free_port(host: &str, port: u16) -> u16 {
	let mut port = port;

	while !is_port_free(host, port) {
		port += 1;

		// This should never happen, but just in case
		if port == 65535 {
			break;
		}
	}

	port
}

pub fn format_address(host: &str, port: u16) -> String {
	format!("http://{host}:{port}")
}
