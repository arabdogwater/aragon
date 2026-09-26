-- Cartoon "paper" card: cream fill, chunky espresso outline and a solid pop
-- shadow underneath (a second frame, since UIStroke can't be offset)

local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App

local Fusion = require(Aragon.Packages.Fusion)

local Theme = require(App.Theme)

local New = Fusion.New
local Children = Fusion.Children

type Props = {
	LayoutOrder: number?,
	Color: Fusion.CanBeState<Color3>?,
	Radius: number?,
	[any]: any,
}

return function(props: Props): Frame
	local radius = UDim.new(0, props.Radius or 14)

	return New "Frame" {
		Name = "Card",
		Size = UDim2.fromScale(1, 0),
		AutomaticSize = Enum.AutomaticSize.Y,
		BackgroundTransparency = 1,
		LayoutOrder = props.LayoutOrder,
		[Children] = {
			New "UIPadding" {
				-- Leaves room for the pop shadow so siblings never overlap it
				PaddingBottom = UDim.new(0, Theme.ShadowOffset),
			},
			New "Frame" {
				Name = "Shadow",
				Position = UDim2.fromOffset(0, Theme.ShadowOffset),
				Size = UDim2.fromScale(1, 1),
				BackgroundColor3 = Theme.Colors.Shadow,
				BorderSizePixel = 0,
				ZIndex = 1,
				[Children] = New "UICorner" {
					CornerRadius = radius,
				},
			},
			New "Frame" {
				Name = "Body",
				Size = UDim2.fromScale(1, 0),
				AutomaticSize = Enum.AutomaticSize.Y,
				BackgroundColor3 = props.Color or Theme.Colors.Background,
				BorderSizePixel = 0,
				ZIndex = 2,
				[Children] = {
					New "UICorner" {
						CornerRadius = radius,
					},
					New "UIStroke" {
						ApplyStrokeMode = Enum.ApplyStrokeMode.Border,
						Color = Theme.Colors.Border,
						Thickness = 2,
					},
					props[Children],
				},
			},
		},
	}
end
