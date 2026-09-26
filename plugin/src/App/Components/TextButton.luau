-- Cartoon button: outlined pill with a solid pop shadow. Lifts on hover,
-- squishes down onto its shadow when pressed (springs, so it feels bouncy)

local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components
local Util = Components.Util

local Fusion = require(Aragon.Packages.Fusion)

local Theme = require(App.Theme)
local getTextSize = require(Util.getTextSize)

local New = Fusion.New
local Value = Fusion.Value
local Spring = Fusion.Spring
local OnEvent = Fusion.OnEvent
local Computed = Fusion.Computed
local Children = Fusion.Children

type Props = {
	Text: Fusion.CanBeState<string>,
	Activated: (() -> ())?,
	Solid: Fusion.CanBeState<boolean>?,
	LayoutOrder: number?,
	Size: Fusion.CanBeState<UDim2>?,
}

local HEIGHT = Theme.CompSizeY.Large
local SHADOW = 3

return function(props: Props): Frame
	local isHovered = Value(false)
	local isPressed = Value(false)

	local lift = Spring(
		Computed(function(use)
			if use(isPressed) then
				return SHADOW
			elseif use(isHovered) then
				return -2
			else
				return 0
			end
		end),
		50,
		0.55
	)

	local width = Computed(function(use)
		local text = use(props.Text)
		return getTextSize(text, Theme.TextSize.Medium, Theme.Fonts.Enums.Display).X + 30
	end)

	local color = Computed(function(use)
		local solid = use(props.Solid)

		if use(isPressed) then
			return if solid then use(Theme.Colors.BrandPress) else use(Theme.Colors.ActiveBackground)
		elseif use(isHovered) then
			return if solid then use(Theme.Colors.BrandHover) else Color3.fromRGB(255, 255, 255)
		else
			return if solid then use(Theme.Colors.Brand) else use(Theme.Colors.Background)
		end
	end)

	return New "Frame" {
		Name = "Button",
		LayoutOrder = props.LayoutOrder,
		BackgroundTransparency = 1,
		Size = props.Size or Computed(function(use)
			return UDim2.fromOffset(use(width), HEIGHT + SHADOW)
		end),
		[Children] = {
			New "Frame" {
				Name = "Shadow",
				Position = UDim2.fromOffset(0, SHADOW),
				Size = UDim2.new(1, 0, 0, HEIGHT),
				BackgroundColor3 = Theme.Colors.Shadow,
				BorderSizePixel = 0,
				ZIndex = 1,
				[Children] = New "UICorner" {
					CornerRadius = UDim.new(0, 12),
				},
			},
			New "TextButton" {
				Position = Computed(function(use)
					return UDim2.fromOffset(0, use(lift))
				end),
				Size = UDim2.new(1, 0, 0, HEIGHT),
				AutoButtonColor = false,
				BackgroundColor3 = Spring(color, 40, 1),
				BorderSizePixel = 0,
				Text = props.Text,
				FontFace = Theme.Fonts.Display,
				TextSize = Theme.TextSize.Medium,
				TextColor3 = Theme.Colors.Text,
				ZIndex = 2,
				[OnEvent "InputBegan"] = function(input)
					if input.UserInputType == Enum.UserInputType.MouseMovement then
						isHovered:set(true)
					elseif input.UserInputType == Enum.UserInputType.MouseButton1 then
						isPressed:set(true)
					end
				end,
				[OnEvent "InputEnded"] = function(input)
					if input.UserInputType == Enum.UserInputType.MouseMovement then
						isHovered:set(false)
						isPressed:set(false)
					elseif input.UserInputType == Enum.UserInputType.MouseButton1 then
						isPressed:set(false)
					end
				end,
				[OnEvent "Activated"] = function()
					isPressed:set(false)

					if props.Activated then
						props.Activated()
					end
				end,
				[Children] = {
					New "UICorner" {
						CornerRadius = UDim.new(0, 12),
					},
					New "UIStroke" {
						ApplyStrokeMode = Enum.ApplyStrokeMode.Border,
						Color = Theme.Colors.Border,
						Thickness = 2,
					},
				},
			},
		},
	}
end
