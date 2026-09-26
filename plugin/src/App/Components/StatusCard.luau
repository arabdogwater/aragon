-- The main block of every connector page: a floating mascot for the state,
-- a status pill with a breathing dot, the title and a short explanation

local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)

local Theme = require(App.Theme)
local clock = require(Components.Util.clock)

local Card = require(Components.Card)
local Container = require(Components.Container)
local Image = require(Components.Image)
local Text = require(Components.Text)
local List = require(Components.List)

local New = Fusion.New
local Value = Fusion.Value
local Computed = Fusion.Computed
local Children = Fusion.Children
local Cleanup = Fusion.Cleanup

type Props = {
	Title: Fusion.CanBeState<string>,
	Body: Fusion.CanBeState<string>?,
	Status: Fusion.CanBeState<string>?,
	-- Status dot color, `Pulse` makes it breathe while waiting
	Color: Fusion.CanBeState<Color3>?,
	Pulse: boolean?,
	-- Mascot illustration for this state
	Mascot: string?,
	LayoutOrder: number?,
}

local MASCOT_SIZE = 84

return function(props: Props): Frame
	local bob = Value(0)
	local tilt = Value(0)
	local pulse = Value(0)

	local unbind = clock.bind(function(now)
		bob:set(math.sin(now * 1.6) * 4)
		tilt:set(math.sin(now * 1.1) * 3)

		if props.Pulse then
			pulse:set((math.sin(now * 3) + 1) / 2)
		end
	end)

	local dotColor = props.Color or Theme.Colors.Brand

	local mascotSize = if props.Mascot then MASCOT_SIZE else 0
	local inset = UDim.new(0, 12)

	return Card {
		LayoutOrder = props.LayoutOrder,
		[Children] = {
			New "UIPadding" {
				PaddingTop = inset,
				PaddingBottom = inset,
				PaddingLeft = inset,
				PaddingRight = inset,
			},
			List {
				FillDirection = Enum.FillDirection.Horizontal,
				VerticalAlignment = Enum.VerticalAlignment.Center,
				Spacing = 10,
			},
			props.Mascot and New "Frame" {
				Name = "MascotSlot",
				Size = UDim2.fromOffset(MASCOT_SIZE, MASCOT_SIZE),
				BackgroundTransparency = 1,
				[Cleanup] = unbind,
				[Children] = Image {
					AnchorPoint = Vector2.new(0.5, 0.5),
					Position = Computed(function(use)
						return UDim2.new(0.5, 0, 0.5, use(bob))
					end),
					Rotation = tilt,
					Size = UDim2.fromScale(1, 1),
					ScaleType = Enum.ScaleType.Fit,
					Image = props.Mascot,
					ZIndex = 3,
				},
			} or New "Folder" {
				[Cleanup] = unbind,
			},
			Container {
				Size = UDim2.new(1, if mascotSize > 0 then -(mascotSize + 10) else 0, 0, 0),
				LayoutOrder = 1,
				[Children] = {
					List {
						Spacing = 4,
					},
					props.Status and New "Frame" {
						Name = "Pill",
						AutomaticSize = Enum.AutomaticSize.XY,
						BackgroundColor3 = Theme.Colors.ActiveBackground,
						BorderSizePixel = 0,
						ZIndex = 3,
						[Children] = {
							New "UICorner" {
								CornerRadius = UDim.new(1, 0),
							},
							New "UIStroke" {
								ApplyStrokeMode = Enum.ApplyStrokeMode.Border,
								Color = Theme.Colors.Border,
								Thickness = 1.5,
							},
							New "UIPadding" {
								PaddingLeft = UDim.new(0, 8),
								PaddingRight = UDim.new(0, 10),
								PaddingTop = UDim.new(0, 2),
								PaddingBottom = UDim.new(0, 2),
							},
							List {
								FillDirection = Enum.FillDirection.Horizontal,
								VerticalAlignment = Enum.VerticalAlignment.Center,
								Spacing = 6,
							},
							New "Frame" {
								Size = UDim2.fromOffset(9, 9),
								BackgroundColor3 = dotColor,
								BorderSizePixel = 0,
								ZIndex = 3,
								[Children] = {
									New "UICorner" {
										CornerRadius = UDim.new(1, 0),
									},
									New "UIStroke" {
										Color = dotColor,
										Thickness = Computed(function(use)
											return 1 + use(pulse) * 3
										end),
										Transparency = Computed(function(use)
											return 0.3 + use(pulse) * 0.7
										end),
									},
								},
							},
							Text {
								Text = props.Status,
								Font = Theme.Fonts.Display,
								TextSize = 13,
								LayoutOrder = 1,
								ZIndex = 3,
							},
						},
					} or nil,
					Text {
						Size = UDim2.fromScale(1, 0),
						AutomaticSize = Enum.AutomaticSize.Y,
						Text = props.Title,
						Font = Theme.Fonts.Display,
						TextSize = 19,
						TextWrapped = true,
						LayoutOrder = 1,
						ZIndex = 3,
					},
					props.Body and Text {
						Size = UDim2.fromScale(1, 0),
						AutomaticSize = Enum.AutomaticSize.Y,
						Text = props.Body,
						Color = Theme.Colors.TextDimmed,
						Font = Theme.Fonts.Bold,
						TextSize = Theme.TextSize.Small,
						TextWrapped = true,
						RichText = true,
						LayoutOrder = 2,
						ZIndex = 3,
					} or nil,
				},
			},
		},
	}
end
