local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)

local Place = require(Aragon.Place)
local Assets = require(App.Assets)
local Theme = require(App.Theme)

local StatusCard = require(Components.StatusCard)
local TextButton = require(Components.TextButton)
local Container = require(Components.Container)
local List = require(Components.List)

local Children = Fusion.Children

type Props = {
	App: { [string]: any },
}

-- New place without a project folder: onboarding happens in the dashboard
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = `Set up {Place.Name}`,
			Status = "Needs a folder",
			Mascot = Assets.Mascots.Setup,
			Body = "This place has no project folder yet. Finish the quick setup in the Aragon dashboard and sync starts right away.",
			Color = Theme.Colors.Warn,
			Pulse = true,
		},
		Container {
			Size = UDim2.fromScale(1, 0),
			LayoutOrder = 1,
			[Children] = {
				List {
					FillDirection = Enum.FillDirection.Horizontal,
					HorizontalAlignment = Enum.HorizontalAlignment.Right,
					Wraps = true,
					Spacing = 8,
				},
				TextButton {
					Text = "Open dashboard",
					Solid = true,
					Activated = function()
						props.App:openDashboard()
					end,
				},
			},
		},
	}
end
