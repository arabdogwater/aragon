local RunService = game:GetService("RunService")
local TweenService = game:GetService("TweenService")

local plugin = script:FindFirstAncestorWhichIsA("Plugin")

local Aragon = script:FindFirstAncestor("Aragon")
local Packages = Aragon.Packages
local Helpers = Aragon.Helpers

local Components = script.Components
local Widgets = script.Widgets
local Pages = script.Pages

local Fusion = require(Packages.Fusion)
local Signal = require(Packages.Signal)

local manifest = require(Aragon.manifest)
local Config = require(Aragon.Config)
local Types = require(Aragon.Types)
local Util = require(Aragon.Util)
local Log = require(Aragon.Log)
local Core = require(Aragon.Core)
local Connector = require(Aragon.Connector)
local describeChanges = require(Helpers.describeChanges)

local Assets = require(script.Assets)
local Loader = require(script.Loader)
local Theme = require(script.Theme)

local Toolbar = require(Components.Plugin.Toolbar)
local ToolbarButton = require(Components.Plugin.ToolbarButton)
local Widget = require(Components.Plugin.Widget)
local Container = require(Components.Container)
local Image = require(Components.Image)
local Text = require(Components.Text)
local List = require(Components.List)
local Scenery = require(Components.Scenery)
local clock = require(Components.Util.clock)

local Searching = require(Pages.Searching)
local Onboarding = require(Pages.Onboarding)
local Paused = require(Pages.Paused)
local Disconnected = require(Pages.Disconnected)
local Connecting = require(Pages.Connecting)
local Connected = require(Pages.Connected)
local Unavailable = require(Pages.Unavailable)
local Prompt = require(Pages.Prompt)
local Error = require(Pages.Error)

local Diff = require(Widgets.Diff)

local New = Fusion.New
local Value = Fusion.Value
local Spring = Fusion.Spring
local OnEvent = Fusion.OnEvent
local OnChange = Fusion.OnChange
local Observer = Fusion.Observer
local Computed = Fusion.Computed
local Children = Fusion.Children
local cleanup = Fusion.cleanup
local peek = Fusion.peek

-- Page layout: padding around the column and the narrowest width it lays out at
-- (below that the column keeps this width and scales down instead)
local PAGE_PADDING = 12
local MIN_LAYOUT_WIDTH = 300

-- How often to look for the hub while it isn't running
local SEARCH_INTERVAL = 2
-- How often to re-check a place that waits for onboarding
local RECHECK_INTERVAL = 10
-- Delay before retrying after a sync error
local RETRY_INTERVAL = 5
-- Minimum time between sync activity reports sent to the dashboard
local SYNC_REPORT_INTERVAL = 5

local App = {}
App.__index = App

function App.new()
	Loader.load()

	local self = setmetatable({}, App)

	self.core = nil
	self.unloading = false
	self.manualPause = false
	self.polling = false
	self.finishSync = nil
	self.lastReport = 0
	self.wake = Signal.new()

	self.connector = Connector.new(Config:get("Host"), Config:get("Port"), Config:get("Https"))

	self.status = Value("Searching")
	self.hello = Value(nil)
	self.lastSync = Value(os.time())
	self.lastSyncKind = Value("Unknown")

	self.rootSize = Value(Vector2.new(300, 200))
	self.icon = Value(Assets.Aragon.Logo)
	self.pages = Value({})

	self.diffWidget = Value(nil)

	local isOpen = Value(false)

	local toolbarButton = ToolbarButton {
		Toolbar = Toolbar {
			Name = "Aragon",
		},
		Name = "Aragon",
		ToolTip = "Aragon sync status. Everything else lives in the Aragon dashboard on your PC",
		Image = self.icon,
		[OnEvent "Click"] = function()
			isOpen:set(not peek(isOpen))
		end,
	}

	Widget {
		Name = "Aragon",
		MinimumSize = peek(self.rootSize),
		Enabled = isOpen,
		[OnChange "Enabled"] = function(isEnabled)
			isOpen:set(isEnabled)
		end,
		[OnChange "AbsoluteSize"] = function(size)
			self.rootSize:set(size)
		end,
		[Children] = {
			Scenery {
				RootSize = self.rootSize,
			},
			self.pages,
		},
	}

	-- Ambient animation only runs while someone can see it
	clock.setActive(peek(isOpen))

	plugin.Unloading:Connect(Observer(isOpen):onChange(function()
		clock.setActive(peek(isOpen))
	end))

	plugin.Unloading:Connect(Observer(isOpen):onChange(function()
		toolbarButton:SetActive(peek(isOpen))
	end))

	plugin.Unloading:Connect(function()
		self.unloading = true
		self:stopCore()
		self.connector:bye()
		self.wake:Fire()
	end)

	toolbarButton:SetActive(peek(isOpen))

	if RunService:IsEdit() then
		self:setPage(Searching {
			App = self,
		})

		task.spawn(function()
			self:runConnector()
		end)
	else
		self:setPage(Unavailable())
	end

	return self
end

function App:setIcon(icon: ("Ok" | "Warn" | "Error")?)
	if icon == "Ok" then
		self.icon:set(Assets.Aragon.LogoOk)
	elseif icon == "Warn" then
		self.icon:set(Assets.Aragon.LogoWarn)
	elseif icon == "Error" then
		self.icon:set(Assets.Aragon.LogoError)
	else
		self.icon:set(Assets.Aragon.Logo)
	end
end

--- Switches the page only when the status actually changes, so the
--- connector can re-check every few seconds without flickering the UI
function App:setStatus(status: string, page: any)
	if peek(self.status) == status then
		return
	end

	self.status:set(status)
	self:setPage(page)
end

function App:setPage(page)
	local pages = peek(self.pages)

	for id, child in pages do
		if child._finished then
			pages[id] = nil
			continue
		end

		task.spawn(function()
			child._destructor(child._value)
			child._finished = true
		end)
	end

	local size = Value(UDim2.fromScale(1, 1))

	-- Extremely hacky way to smoothly transition between pages
	page = Computed(function()
		-- Natural height of the column at scale 1
		local contentHeight = Value(0)

		-- Lay the column out at a comfortable width, then scale the whole thing
		-- (UIScale) so every element always fits the widget, never scrolls/hides
		local layoutWidth = Computed(function(use)
			return math.max(use(self.rootSize).X - PAGE_PADDING * 2, MIN_LAYOUT_WIDTH)
		end)

		local scale = Computed(function(use)
			local root = use(self.rootSize)
			local height = use(contentHeight)
			local width = use(layoutWidth)

			if height <= 0 then
				return 1
			end

			local fitX = (root.X - PAGE_PADDING * 2) / width
			local fitY = (root.Y - PAGE_PADDING * 2) / height

			return math.clamp(math.min(fitX, fitY), 0.2, 1)
		end)

		local uiScale = New "UIScale" {
			Scale = scale,
		}

		return New "Frame" {
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.fromScale(0.5, 0.5),
			Size = Spring(size, 10),
			BackgroundTransparency = 1,
			BorderSizePixel = 0,
			ZIndex = 5,
			[Children] = New "Frame" {
				Name = "Column",
				AnchorPoint = Vector2.new(0.5, 0.5),
				Position = UDim2.fromScale(0.5, 0.5),
				Size = Computed(function(use)
					return UDim2.fromOffset(use(layoutWidth), use(contentHeight))
				end),
				BackgroundTransparency = 1,
				[Children] = {
					uiScale,
					List {
						HorizontalAlignment = Enum.HorizontalAlignment.Center,
						Spacing = 10,
						[OnChange "AbsoluteContentSize"] = function(contentSize)
							-- AbsoluteContentSize is post-scale; divide by the scale that was
							-- actually applied during this layout to get layout units back
							contentHeight:set(math.ceil(contentSize.Y / math.max(uiScale.Scale, 0.01)))
						end,
					},
					Container {
						Size = UDim2.fromScale(1, 0),
						[Children] = {
							Image {
								Size = UDim2.fromOffset(178, 48),
								Image = Assets.Aragon.Banner,
								ScaleType = Enum.ScaleType.Fit,
							},
							Text {
								AnchorPoint = Vector2.new(1, 0.5),
								Position = UDim2.new(1, -2, 0.5, 0),
								Text = `v{manifest.package.version}`,
								Font = Theme.Fonts.Display,
								TextSize = 13,
								BackgroundTransparency = 0,
								BackgroundColor3 = Theme.Colors.Background,
								[Children] = {
									New "UICorner" {
										CornerRadius = UDim.new(1, 0),
									},
									New "UIStroke" {
										ApplyStrokeMode = Enum.ApplyStrokeMode.Border,
										Color = Theme.Colors.Border,
										Thickness = 1.5,
									},
									New "UIPadding" {
										PaddingLeft = UDim.new(0, 8),
										PaddingRight = UDim.new(0, 8),
										PaddingTop = UDim.new(0, 2),
										PaddingBottom = UDim.new(0, 2),
									},
								},
							},
						},
					},
					Container {
						Size = UDim2.fromScale(1, 0),
						LayoutOrder = 1,
						[Children] = page,
					},
				},
			},
		}
	end, function(page)
		if not page then
			return
		end

		local TWEEN_INFO = TweenInfo.new(0.2, Enum.EasingStyle.Sine, Enum.EasingDirection.Out)
		TweenService:Create(page, TWEEN_INFO, { BackgroundTransparency = 1 }):Play()

		for _, descendant in page:GetDescendants() do
			local properties = {}

			if descendant:IsA("GuiObject") then
				if descendant:IsA("GuiButton") then
					descendant.Active = false
				end

				properties.BackgroundTransparency = 1

				if descendant:IsA("TextLabel") or descendant:IsA("TextButton") or descendant:IsA("TextBox") then
					properties.TextTransparency = 1
				elseif descendant:IsA("ImageLabel") or descendant:IsA("ImageButton") then
					properties.ImageTransparency = 1
				end
			end

			if descendant:IsA("UIStroke") then
				properties.Transparency = 1
			end

			if next(properties) then
				TweenService:Create(descendant, TWEEN_INFO, properties):Play()
			end
		end

		page.ZIndex = 2

		size:set(UDim2.fromScale(1.2, 1))

		task.wait(0.2)
		cleanup(page)
	end)

	pages[Util.generateGUID()] = page
	self.pages:set(pages)
end

--- Waits for `seconds` unless something wakes the connector up earlier
function App:sleep(seconds: number)
	local resumed = false
	local thread = coroutine.running()

	local function resume()
		if not resumed then
			resumed = true
			task.spawn(thread)
		end
	end

	local connection = self.wake:Once(resume)
	task.delay(seconds, resume)

	coroutine.yield()
	connection:Unbind()
end

--- The connector loop: find the hub, introduce this place, then either sync
--- or wait for the dashboard (onboarding / paused), forever
function App:runConnector()
	while not self.unloading do
		if self.manualPause then
			self.wake:Wait()
			continue
		end

		local ok, response = self.connector:hello():await()

		if self.unloading or self.manualPause then
			continue
		end

		if not ok then
			self.hello:set(nil)
			self:setIcon()
			self:setStatus("Searching", Searching {
				App = self,
			})

			self:sleep(SEARCH_INTERVAL)
			continue
		end

		Config:setHub(response.settings or {})
		self.hello:set(response)
		self:startPolling()

		if response.state == "Mapped" then
			local reason = self:sync(response)

			if reason == "error" then
				self:sleep(RETRY_INTERVAL)
			elseif reason == "lost" then
				self:sleep(SEARCH_INTERVAL)
			end
		elseif response.state == "Onboarding" then
			self:setIcon("Warn")
			self:setStatus("Onboarding", Onboarding {
				App = self,
			})

			self:sleep(RECHECK_INTERVAL)
		elseif response.state == "Paused" then
			self:setIcon()
			self:setStatus("Paused", Paused {
				App = self,
			})

			self:sleep(RECHECK_INTERVAL * 3)
		else
			self:setIcon("Error")
			self:setStatus(
				"Error",
				Error {
					App = self,
					Message = response.message or "The project for this place failed to load",
				}
			)

			self:sleep(RETRY_INTERVAL * 2)
		end
	end
end

--- Keeps the control channel to the hub open (it doubles as the heartbeat)
function App:startPolling()
	if self.polling then
		return
	end

	self.polling = true

	task.spawn(function()
		while not self.unloading do
			local ok, controls = self.connector:poll():await()

			if not ok then
				-- Hub restarted or closed, the connector loop says hello again
				self.polling = false

				if not self.core then
					self.wake:Fire()
				end

				return
			end

			for _, control in controls or {} do
				self:handleControl(control)
			end
		end

		self.polling = false
	end)
end

function App:handleControl(control: Connector.Control)
	Log.trace("Hub control:", control.kind)

	if control.kind == "Settings" then
		Config:setHub(control.settings or {})
	elseif control.kind == "Reconnect" then
		self:restart()
	elseif control.kind == "Disconnect" then
		Log.info(control.message or "Disconnected by the Aragon dashboard")
		self:restart()
	elseif control.kind == "Toast" and control.message then
		Log.info(control.message)
	end
end

--- Drops the current sync (if any) and says hello again right away
function App:restart()
	self:stopCore()

	if self.finishSync then
		self.finishSync("restart")
	end

	self.wake:Fire()
end

function App:stopCore()
	if self.core then
		self.core:stop()
		self.core = nil
	end
end

--- Runs one sync session and yields until it ends. Returns why it ended:
--- "done" | "restart" | "declined" | "lost" | "error"
function App:sync(response: Connector.HelloResponse): string
	local done = Signal.new()
	local finished = false
	local project = Value({ name = response.projectName or "" })

	local function finish(reason: string)
		if finished then
			return
		end

		finished = true
		self.finishSync = nil
		done:Fire(reason)
	end

	self.finishSync = finish

	local core = Core.new(Config:get("Host"), Config:get("Port"), response.base)
	self.core = core

	task.delay(0.25, function()
		if not finished and core.status == Core.Status.Connecting and peek(self.status) ~= "Prompt" then
			self:setStatus("Connecting", Connecting {
				App = self,
			})
		end
	end)

	local function showConnected()
		self:setIcon("Ok")
		self:setStatus("Connected", Connected {
			App = self,
			Project = project,
		})
	end

	core:onPrompt(function(message, changes, tree)
		local signal = Signal.new()
		local result = false

		self:setIcon("Warn")
		self.status:set("Prompt")

		-- Prompt user to accept or reject incoming changes
		if changes then
			self:setPage(Prompt {
				App = self,
				Message = describeChanges(changes),
				Options = { "Accept", "Diff", "Cancel" },
				Signal = signal,
			})

			local choice = signal:Wait()

			while choice == "Diff" do
				self:diff(changes, tree)
				choice = signal:Wait()
			end

			self:diff()

			result = choice == "Accept"
		-- Prompt user whether to continue when the project IDs differ
		else
			self:setPage(Prompt {
				App = self,
				Message = message,
				Options = { "Continue", "Cancel" },
				Signal = signal,
			})

			result = signal:Wait() == "Continue"
		end

		if core.status == Core.Status.Connected then
			showConnected()
		end

		return result
	end)

	core:onReady(function(projectDetails)
		project:set(projectDetails)

		self.lastSync:set(os.time())
		self.lastSyncKind:set(response.initialSyncPriority or Config:get("InitialSyncPriority"))

		showConnected()
		self.connector:synced(response.placeKey)
	end)

	core:onSync(function(kind, data)
		self.lastSync:set(os.time())
		self.lastSyncKind:set(
			(kind == "SyncChanges" or kind == "SyncDetails") and "Server"
				or kind == "SyncbackChanges" and "Client"
				or "None"
		)

		if kind == "SyncDetails" then
			project:set(data)
		end

		if os.clock() - self.lastReport > SYNC_REPORT_INTERVAL then
			self.lastReport = os.clock()
			self.connector:synced(response.placeKey)
		end
	end)

	core:run(response.initialSyncPriority)
		:andThen(function()
			finish("done")
		end)
		:catch(function(err)
			if finished then
				return
			end

			local kind = type(err) == "table" and err.kind or nil

			if kind == "Disconnected" then
				-- The hub remapped or paused this place, say hello again
				finish("restart")
			elseif kind == "ConnectFail" or kind == "ConnectionClosed" then
				finish("lost")
			elseif Core.wasExitGraceful(err) then
				-- A prompt was declined: stay offline until the user reconnects
				self.manualPause = true
				self:setIcon()
				self:setStatus(
					"Disconnected",
					Disconnected {
						App = self,
						Message = "Sync cancelled. Nothing was changed.",
					}
				)
				finish("declined")
			else
				self:setIcon("Error")
				self:setStatus(
					"Error",
					Error {
						App = self,
						Message = err.message or tostring(err),
					}
				)
				finish("error")
			end
		end)

	local reason = done:Wait()

	if self.core == core then
		self:stopCore()
	end

	return reason
end

--- Disconnect button: stay offline until the user connects again
function App:disconnect()
	self.manualPause = true
	self:restart()

	self:setIcon()
	self:setStatus("Disconnected", Disconnected {
		App = self,
	})
end

--- Connect / retry button
function App:connect()
	self.manualPause = false
	self.status:set("Retry")
	self:setStatus("Searching", Searching {
		App = self,
	})

	self.wake:Fire()
end

function App:openDashboard()
	self.connector:focusDashboard()
end

function App:diff(changes: Types.Changes?, tree: any?)
	local widget = peek(self.diffWidget)

	if widget then
		widget:Destroy()
		widget = nil
	elseif changes and tree then
		widget = Diff {
			Changes = changes,
			Tree = tree,
			Closed = function()
				peek(self.diffWidget):Destroy()
				self.diffWidget:set(nil)
			end,
		}
	end

	self.diffWidget:set(widget)
end

function App:isConnected()
	return self.core and self.core.status == Core.Status.Connected
end

return App
