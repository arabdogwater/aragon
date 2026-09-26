local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)
local Signal = require(Aragon.Packages.Signal)

local Assets = require(App.Assets)
local Theme = require(App.Theme)

local StatusCard = require(Components.StatusCard)
local TextButton = require(Components.TextButton)
local Container = require(Components.Container)
local Padding = require(Components.Padding)
local Text = require(Components.Text)
local List = require(Components.List)
local Box = require(Components.Box)

local ForPairs = Fusion.ForPairs
local Children = Fusion.Children
local cleanup = Fusion.cleanup

type Props = {
	App: { [string]: any },
	Message: string,
	Options: { string },
	Signal: Signal.Signal,
}

return function(props: Props): { Instance }
	return {
		List {
			HorizontalAlignment = Enum.HorizontalAlignment.Right,
		},
		StatusCard {
			Title = "Hold up, check this",
			Status = "Needs you",
			Body = "Big changes incoming. Take a look before anything is overwritten.",
			Color = Theme.Colors.Warn,
			Pulse = true,
			Mascot = Assets.Mascots.Search,
		},
		Box {
			LayoutOrder = 1,
			Size = UDim2.fromScale(1, 0),
			AutomaticSize = Enum.AutomaticSize.Y,
			[Children] = {
				Padding {},
				Text {
					Size = UDim2.fromScale(1, 0),
					AutomaticSize = Enum.AutomaticSize.Y,
					Text = props.Message,
					RichText = true,
					TextWrapped = true,
					Font = Theme.Fonts.Mono,
					TextSize = Theme.TextSize.Small,
				},
			},
		},
		Container {
			Size = UDim2.fromScale(1, 0),
			LayoutOrder = 2,
			[Children] = {
				List {
					FillDirection = Enum.FillDirection.Horizontal,
					HorizontalAlignment = Enum.HorizontalAlignment.Right,
					Wraps = true,
					Spacing = 8,
				},
				ForPairs(props.Options, function(_, index, option)
					return index,
						TextButton {
							LayoutOrder = #props.Options - index + 1,
							Solid = index == 1,
							Text = option,
							Activated = function()
								props.Signal:Fire(option)
							end,
						}
				end, cleanup),
			},
		},
	}
end
