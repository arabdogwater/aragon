//! Aragon lives in the system tray. The hub syncs in the background; the
//! dashboard (a WebView2 window) only exists while it's open. Closing it drops
//! the webview, which shuts the WebView2 processes down, so the idle cost is
//! just the hub itself.

use anyhow::Result;
use log::{info, warn};
use std::{path::PathBuf, sync::Arc};
use tao::{
	dpi::LogicalSize,
	event::{Event, StartCause, WindowEvent},
	event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget},
	window::{Icon, Theme, Window, WindowBuilder},
};
use tray_icon::{
	menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
	MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use wry::{WebContext, WebView, WebViewBuilder};

use super::Hub;

/// 64x64 RGBA icon, generated from the brand mark
const ICON_RGBA: &[u8] = include_bytes!("../../assets/branding/icon-64.rgba");

const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";

enum UserEvent {
	Open,
	Tray(TrayIconEvent),
	Menu(MenuEvent),
}

/// Field order matters: the webview must drop before its window and context
struct Dashboard {
	webview: WebView,
	window: Window,
	_context: WebContext,
}

fn open_dashboard(target: &EventLoopWindowTarget<UserEvent>, url: &str) -> Result<Option<Dashboard>> {
	let window = WindowBuilder::new()
		.with_title("Aragon")
		.with_inner_size(LogicalSize::new(1320.0, 860.0))
		.with_min_inner_size(LogicalSize::new(880.0, 600.0))
		.with_window_icon(Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok())
		.with_theme(Some(Theme::Light))
		.build(target)?;

	// `ARAGON_WEBVIEW_DIR` isolates the WebView2 profile (dev/testing next to a running instance)
	let data_dir = std::env::var_os("ARAGON_WEBVIEW_DIR")
		.map(PathBuf::from)
		.unwrap_or_else(|| {
			std::env::var_os("LOCALAPPDATA")
				.map(PathBuf::from)
				.unwrap_or_else(std::env::temp_dir)
				.join("Aragon")
				.join("webview")
		});

	let mut context = WebContext::new(Some(data_dir));

	// WebView2 composites on the GPU by default, no extra flags needed
	let builder = WebViewBuilder::new_with_web_context(&mut context)
		.with_url(url)
		.with_devtools(cfg!(debug_assertions) || std::env::var_os("ARAGON_DEVTOOLS").is_some())
		.with_background_color((255, 241, 191, 255));

	#[cfg(windows)]
	let builder = {
		use wry::WebViewBuilderExtWindows;
		builder
			.with_theme(wry::Theme::Light)
			.with_browser_accelerator_keys(false)
	};

	match builder.build(&window) {
		Ok(webview) => Ok(Some(Dashboard {
			webview,
			window,
			_context: context,
		})),
		Err(err) => {
			// No WebView2 runtime: the dashboard still works in any browser
			warn!("Failed to create webview ({err}), opening the dashboard in the browser instead");
			open::that(url).ok();
			Ok(None)
		}
	}
}

/// Asks WebView2 to trim memory while the dashboard is in the background
#[cfg(windows)]
fn set_low_memory(webview: &WebView, low: bool) {
	use webview2_com::Microsoft::Web::WebView2::Win32::{
		ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
	};
	use windows_core::Interface;
	use wry::WebViewExtWindows;

	let level = if low {
		COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
	} else {
		COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
	};

	// SAFETY: plain COM calls on the live controller, on the UI thread
	unsafe {
		if let Ok(core) = webview.controller().CoreWebView2() {
			if let Ok(core) = core.cast::<ICoreWebView2_19>() {
				core.SetMemoryUsageTargetLevel(level).ok();
			}
		}
	}
}

#[cfg(not(windows))]
fn set_low_memory(_webview: &WebView, _low: bool) {}

fn build_tray() -> Result<TrayIcon> {
	let open = MenuItem::with_id(MENU_OPEN, "Open dashboard", true, None);
	let quit = MenuItem::with_id(MENU_QUIT, "Quit Aragon", true, None);
	let menu = Menu::with_items(&[&open, &PredefinedMenuItem::separator(), &quit])?;

	let tray = TrayIconBuilder::new()
		.with_tooltip("Aragon is syncing. Click to open the dashboard")
		.with_icon(tray_icon::Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64)?)
		.with_menu(Box::new(menu))
		.with_menu_on_left_click(false)
		.build()?;

	Ok(tray)
}

/// Runs the tray + on-demand dashboard on the main thread (never returns)
pub fn run(hub: Arc<Hub>, dev_url: Option<String>, open_now: bool) -> Result<()> {
	let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

	let proxy = event_loop.create_proxy();
	hub.set_attention(move || {
		proxy.send_event(UserEvent::Open).ok();
	});

	let proxy = event_loop.create_proxy();
	TrayIconEvent::set_event_handler(Some(move |event| {
		proxy.send_event(UserEvent::Tray(event)).ok();
	}));

	let proxy = event_loop.create_proxy();
	MenuEvent::set_event_handler(Some(move |event| {
		proxy.send_event(UserEvent::Menu(event)).ok();
	}));

	let url = dev_url.unwrap_or_else(|| format!("http://127.0.0.1:{}/ui/", hub.port));
	let mut dashboard: Option<Dashboard> = None;
	let mut tray: Option<TrayIcon> = None;

	event_loop.run(move |event, target, control_flow| {
		*control_flow = ControlFlow::Wait;

		let show = |dashboard: &mut Option<Dashboard>| {
			if let Some(existing) = dashboard {
				set_low_memory(&existing.webview, false);
				existing.window.set_visible(true);
				existing.window.set_minimized(false);
				existing.window.set_focus();
				return;
			}

			match open_dashboard(target, &url) {
				Ok(created) => *dashboard = created,
				Err(err) => warn!("Failed to open the dashboard: {err}"),
			}
		};

		match event {
			Event::NewEvents(StartCause::Init) => {
				// The tray has to be created once the event loop is running
				match build_tray() {
					Ok(icon) => tray = Some(icon),
					Err(err) => warn!("Failed to create the tray icon: {err}"),
				}

				// Without a tray there'd be no way back in, so always show the window
				if open_now || tray.is_none() {
					show(&mut dashboard);
				}
			}
			Event::UserEvent(UserEvent::Open) => show(&mut dashboard),
			Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
				button: MouseButton::Left,
				button_state: MouseButtonState::Up,
				..
			}))
			| Event::UserEvent(UserEvent::Tray(TrayIconEvent::DoubleClick { .. })) => show(&mut dashboard),
			Event::UserEvent(UserEvent::Menu(event)) => {
				if event.id == MENU_OPEN {
					show(&mut dashboard);
				} else if event.id == MENU_QUIT {
					hub.save();
					dashboard = None;
					tray = None;
					*control_flow = ControlFlow::Exit;
				}
			}
			Event::WindowEvent {
				event: WindowEvent::Focused(focused),
				..
			} => {
				// Background/minimized dashboard -> WebView2 low memory target
				if let Some(existing) = &dashboard {
					set_low_memory(&existing.webview, !focused);
				}
			}
			Event::WindowEvent {
				event: WindowEvent::CloseRequested,
				..
			} => {
				// Free WebView2 entirely; the hub keeps syncing from the tray
				dashboard = None;
				hub.save();
				info!("Dashboard closed, Aragon keeps syncing in the tray");

				if tray.is_none() {
					*control_flow = ControlFlow::Exit;
				}
			}
			_ => {}
		}
	});
}
