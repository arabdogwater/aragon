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
	Message: string?,
}

-- The user disconnected (or declined a prompt): stay offline until asked
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Disconnected",
			Status = "Offline",
			Mascot = Assets.Mascots.Wave,
			Body = props.Message or "Auto-connect is on hold for this session.",
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
					Text = "Connect",
					Solid = true,
					Activated = function()
						props.App:connect()
					end,
				},
			},
		},
	}
end
