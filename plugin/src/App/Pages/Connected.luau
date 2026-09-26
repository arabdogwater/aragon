local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)

local Types = require(Aragon.Types)
local Place = require(Aragon.Place)
local Assets = require(App.Assets)
local Theme = require(App.Theme)

local StatusCard = require(Components.StatusCard)
local TextButton = require(Components.TextButton)
local Container = require(Components.Container)
local List = require(Components.List)

local Value = Fusion.Value
local Observer = Fusion.Observer
local Computed = Fusion.Computed
local Children = Fusion.Children
local Cleanup = Fusion.Cleanup
local peek = Fusion.peek

type Props = {
	App: { [string]: any },
	Project: Fusion.Value<Types.Project>,
}

local function formatElapsed(elapsed: number): (string, number)
	if elapsed < 1 then
		return "just now", 1
	elseif elapsed < 60 then
		return math.floor(elapsed) .. "s ago", 1
	elseif elapsed < 3600 then
		return elapsed // 60 .. "m ago", 60
	elseif elapsed < 86400 then
		return elapsed // 3600 .. "h ago", 3600
	else
		return elapsed // 86400 .. "d ago", 86400
	end
end

return function(props: Props): { Instance }
	local syncedText = Value("just now")

	local function updateSyncedText()
		return task.spawn(function()
			while true do
				local text, interval = formatElapsed(os.time() - peek(props.App.lastSync))
				syncedText:set(text)
				task.wait(interval)
			end
		end)
	end

	local thread = updateSyncedText()
	local disconnect = Observer(props.App.lastSync):onChange(function()
		task.cancel(thread)
		thread = updateSyncedText()
	end)

	return {
		List {},
		StatusCard {
			Title = Computed(function(use)
				local name = use(props.Project).name
				return if name and name ~= "" then name else Place.Name
			end),
			Body = Computed(function(use)
				local kind = use(props.App.lastSyncKind)
				local arrow = if kind == "Server" then "↓ " elseif kind == "Client" then "↑ " else ""

				return `Syncing {Place.Name}\n{arrow}Last sync {use(syncedText)}`
			end),
			Color = Theme.Colors.Ok,
			Status = "Syncing",
			Pulse = true,
			Mascot = Assets.Mascots.Syncing,
		},
		Container {
			Size = UDim2.fromScale(1, 0),
			LayoutOrder = 1,
			[Cleanup] = function()
				disconnect()
				task.cancel(thread)
			end,
			[Children] = {
				List {
					FillDirection = Enum.FillDirection.Horizontal,
					HorizontalAlignment = Enum.HorizontalAlignment.Right,
					Wraps = true,
					Spacing = 8,
				},
				TextButton {
					Text = "Disconnect",
					LayoutOrder = 2,
					Activated = function()
						props.App:disconnect()
					end,
				},
				TextButton {
					Text = "Dashboard",
					Solid = true,
					LayoutOrder = 1,
					Activated = function()
						props.App:openDashboard()
					end,
				},
			},
		},
	}
end
