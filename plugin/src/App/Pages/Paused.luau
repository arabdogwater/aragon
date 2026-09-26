local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)

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

-- Sync for this place was turned off in the dashboard
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Sync paused",
			Status = "Paused",
			Mascot = Assets.Mascots.Sleepy,
			Body = "Sync for this place is turned off in the Aragon dashboard.",
			Color = Theme.Colors.TextDimmed,
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
					Activated = function()
						props.App:openDashboard()
					end,
				},
			},
		},
	}
end
