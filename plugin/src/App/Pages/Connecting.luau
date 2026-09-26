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
local Computed = Fusion.Computed

type Props = {
	App: { [string]: any },
	-- What the first sync is doing right now
	Step: Fusion.Value<string>?,
}

-- Place is mapped, the first sync is being prepared
return function(props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Fetching your code",
			Status = "Connecting",
			Body = Computed(function(use)
				local step = props.Step and use(props.Step)
				return if step then `{step}…` else `Sniffing out the project for {Place.Name}…`
			end),
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
