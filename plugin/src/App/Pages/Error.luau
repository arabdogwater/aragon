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
	Message: string,
}

-- Sync failed; the connector retries on its own, Retry just does it now
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Something broke",
			Status = "Error",
			Body = `{props.Message}\n<i>Retrying by itself in a few seconds.</i>`,
			Color = Theme.Colors.Error,
			Mascot = Assets.Mascots.Fix,
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
					Text = "Retry now",
					Solid = true,
					Activated = function()
						props.App:connect()
					end,
				},
			},
		},
	}
end
