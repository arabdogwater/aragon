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

-- Place is mapped, the first sync is being prepared
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Fetching your code",
			Status = "Connecting",
			Body = `Sniffing out the project for {Place.Name}…`,
			Color = Theme.Colors.Brand,
			Pulse = true,
			Mascot = Assets.Mascots.Search,
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
					Text = "Cancel",
					Activated = function()
						props.App:disconnect()
					end,
				},
			},
		},
	}
end
