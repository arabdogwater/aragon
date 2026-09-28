//! Aragon lives in the system tray. The hub syncs in the background; the
//! dashboard (a WebView2 window) only costs anything while you look at it:
//!
//! - closed: the webview is dropped, no WebView2 processes run at all
//! - minimized: the webview is hidden and suspended right away (no CPU), and
//!   after [`HIBERNATE_AFTER`] it's dropped entirely; restoring rebuilds it on
//!   the same page
//! - in the background: WebView2 is asked to keep its memory low
//!
//! The page itself is static when idle (no looping animations), so an open,
//! untouched dashboard renders zero frames.

use anyhow::Result;
use log::{info, warn};
use std::{
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};
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

/// A minimized dashboard is dropped (0 MB) after this long
const HIBERNATE_AFTER: Duration = Duration::from_secs(30);

enum UserEvent {
	Open,
	Tray(TrayIconEvent),
	Menu(MenuEvent),
}

/// Field order matters: the webview must drop before its window and context
struct Dashboard {
	webview: Option<WebView>,
	window: Window,
	context: WebContext,
	/// Page to come back to after hibernating
	url: String,
	minimized_at: Option<Instant>,
}

impl Dashboard {
	fn hibernate_deadline(&self) -> Option<Instant> {
		match (&self.webview, self.minimized_at) {
			(Some(_), Some(at)) => Some(at + HIBERNATE_AFTER),
			_ => None,
		}
	}

	fn on_minimized(&mut self, minimized: bool) {
		if minimized == self.minimized_at.is_some() {
			return;
		}

		if minimized {
			self.minimized_at = Some(Instant::now());

			if let Some(webview) = &self.webview {
				suspend(webview);
			}
		} else {
			self.minimized_at = None;

			match &self.webview {
				Some(webview) => resume(webview),
				None => self.wake(),
			}
		}
	}

	/// Drops the webview (all WebView2 processes exit), keeps the window
	fn hibernate(&mut self) {
		if let Some(webview) = self.webview.take() {
			if let Ok(url) = webview.url() {
				self.url = url;
			}

			info!("Dashboard minimized for a while, released WebView2");
		}
	}

	/// Rebuilds a hibernated webview on the page it was showing
	fn wake(&mut self) {
		if self.webview.is_some() {
			return;
		}

		match build_webview(&self.window, &mut self.context, &self.url) {
			Ok(webview) => self.webview = Some(webview),
			Err(err) => warn!("Failed to restore the dashboard: {err}"),
		}
	}
}

fn data_dir() -> PathBuf {
	// `ARAGON_WEBVIEW_DIR` isolates the WebView2 profile (dev/testing next to a running instance)
	std::env::var_os("ARAGON_WEBVIEW_DIR")
		.map(PathBuf::from)
		.unwrap_or_else(|| {
			std::env::var_os("LOCALAPPDATA")
				.map(PathBuf::from)
				.unwrap_or_else(std::env::temp_dir)
				.join("Aragon")
				.join("webview")
		})
}

fn build_webview(window: &Window, context: &mut WebContext, url: &str) -> wry::Result<WebView> {
	// WebView2 composites on the GPU by default, no extra flags needed
	let builder = WebViewBuilder::new_with_web_context(context)
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

	let webview = builder.build(window)?;
	tune(&webview);

	Ok(webview)
}

fn open_dashboard(target: &EventLoopWindowTarget<UserEvent>, url: &str) -> Result<Option<Dashboard>> {
	let builder = WindowBuilder::new()
		.with_title("Aragon")
		.with_inner_size(LogicalSize::new(1320.0, 860.0))
		.with_min_inner_size(LogicalSize::new(880.0, 600.0))
		.with_window_icon(Icon::from_rgba(ICON_RGBA.to_vec(), 64, 64).ok())
		.with_theme(Some(Theme::Light));

	// `ARAGON_BENCH`: real window for resource measurements, but off-screen,
	// unfocused and out of the taskbar so it never shows up on the desktop
	let builder = if std::env::var_os("ARAGON_BENCH").is_some() {
		#[cfg(windows)]
		let builder = {
			use tao::platform::windows::WindowBuilderExtWindows;
			builder.with_skip_taskbar(true)
		};

		builder
			.with_position(tao::dpi::PhysicalPosition::new(-6000, -6000))
			.with_focused(false)
	} else {
		builder
	};

	let window = builder.build(target)?;
	let mut context = WebContext::new(Some(data_dir()));

	match build_webview(&window, &mut context, url) {
		Ok(webview) => Ok(Some(Dashboard {
			webview: Some(webview),
			window,
			context,
			url: url.to_owned(),
			minimized_at: None,
		})),
		Err(err) if std::env::var_os("ARAGON_BENCH").is_some() => {
			warn!("Failed to create webview ({err})");
			Ok(None)
		}
		Err(err) => {
			// No WebView2 runtime: the dashboard still works in any browser
			warn!("Failed to create webview ({err}), opening the dashboard in the browser instead");
			open::that(url).ok();
			Ok(None)
		}
	}
}

/// Turns off browser features a local dashboard never uses, so WebView2 does
/// no background work for them (SmartScreen lookups, autofill, gestures)
#[cfg(windows)]
fn tune(webview: &WebView) {
	use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Settings6, ICoreWebView2Settings8};
	use windows_core::Interface;
	use wry::WebViewExtWindows;

	// SAFETY: plain COM calls on the live controller, on the UI thread
	unsafe {
		let Ok(core) = webview.controller().CoreWebView2() else {
			return;
		};
		let Ok(settings) = core.Settings() else {
			return;
		};

		settings.SetIsStatusBarEnabled(false).ok();

		if let Ok(settings) = settings.cast::<ICoreWebView2Settings6>() {
			settings.SetIsPasswordAutosaveEnabled(false).ok();
			settings.SetIsGeneralAutofillEnabled(false).ok();
			settings.SetIsPinchZoomEnabled(false).ok();
			settings.SetIsSwipeNavigationEnabled(false).ok();
		}

		if let Ok(settings) = settings.cast::<ICoreWebView2Settings8>() {
			// The dashboard only loads from localhost, no reputation checks needed
			settings.SetIsReputationCheckingRequired(false).ok();
		}
	}
}

#[cfg(not(windows))]
fn tune(_webview: &WebView) {}

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

/// Hidden + suspended: the renderer stops running scripts and painting
#[cfg(windows)]
fn suspend(webview: &WebView) {
	use webview2_com::{Microsoft::Web::WebView2::Win32::ICoreWebView2_3, TrySuspendCompletedHandler};
	use windows_core::Interface;
	use wry::WebViewExtWindows;

	set_low_memory(webview, true);
	webview.set_visible(false).ok();

	// SAFETY: plain COM calls on the live controller, on the UI thread
	unsafe {
		if let Ok(core) = webview.controller().CoreWebView2() {
			if let Ok(core) = core.cast::<ICoreWebView2_3>() {
				let handler = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
				core.TrySuspend(&handler).ok();
			}
		}
	}
}

#[cfg(windows)]
fn resume(webview: &WebView) {
	use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
	use windows_core::Interface;
	use wry::WebViewExtWindows;

	// SAFETY: plain COM calls on the live controller, on the UI thread
	unsafe {
		if let Ok(core) = webview.controller().CoreWebView2() {
			if let Ok(core) = core.cast::<ICoreWebView2_3>() {
				core.Resume().ok();
			}
		}
	}

	webview.set_visible(true).ok();
	set_low_memory(webview, false);
}

#[cfg(not(windows))]
fn suspend(webview: &WebView) {
	webview.set_visible(false).ok();
}

#[cfg(not(windows))]
fn resume(webview: &WebView) {
	webview.set_visible(true).ok();
}

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
		let show = |dashboard: &mut Option<Dashboard>| {
			if let Some(existing) = dashboard {
				existing.window.set_visible(true);
				existing.window.set_minimized(false);
				existing.on_minimized(false);
				existing.wake();

				if let Some(webview) = &existing.webview {
					set_low_memory(webview, false);
				}

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
			Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
				if let Some(existing) = &mut dashboard {
					if existing
						.hibernate_deadline()
						.is_some_and(|deadline| Instant::now() >= deadline)
					{
						existing.hibernate();
					}
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
					return;
				}
			}
			Event::WindowEvent {
				event: WindowEvent::Resized(_),
				..
			} => {
				if let Some(existing) = &mut dashboard {
					let minimized = existing.window.is_minimized();
					existing.on_minimized(minimized);
				}
			}
			Event::WindowEvent {
				event: WindowEvent::Focused(focused),
				..
			} => {
				// Background dashboard -> WebView2 low memory target
				if let Some(webview) = dashboard.as_ref().and_then(|existing| existing.webview.as_ref()) {
					set_low_memory(webview, !focused);
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
					return;
				}
			}
			_ => {}
		}

		// Sleep until the next hibernation check, or until something happens
		*control_flow = match dashboard.as_ref().and_then(Dashboard::hibernate_deadline) {
			Some(deadline) => ControlFlow::WaitUntil(deadline),
			None => ControlFlow::Wait,
		};
	});
}
