-- The living shiba town behind every page: the illustrated village fills the
-- widget (bottom anchored, like CSS `cover`), clouds drift across the sky at
-- different depths and two bees buzz around the flowers.

local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Fusion = require(Aragon.Packages.Fusion)

local Assets = require(App.Assets)
local Theme = require(App.Theme)
local clock = require(Components.Util.clock)

local Image = require(Components.Image)

local New = Fusion.New
local Value = Fusion.Value
local Computed = Fusion.Computed
local Children = Fusion.Children
local Cleanup = Fusion.Cleanup

-- Town art is 1024x580
local TOWN_ASPECT = 1024 / 580

type Props = {
	RootSize: Fusion.Value<Vector2>,
}

-- Clouds: vertical position (scale), width (px), speed (widths per second), phase
local CLOUDS = {
	{ y = 0.03, width = 118, speed = 0.018, phase = 0.1, image = "CloudA", transparency = 0.05 },
	{ y = 0.13, width = 70, speed = 0.011, phase = 0.55, image = "CloudB", transparency = 0.2 },
	{ y = 0.22, width = 56, speed = 0.008, phase = 0.8, image = "CloudA", transparency = 0.3 },
	{ y = 0.08, width = 92, speed = 0.014, phase = 0.35, image = "CloudB", transparency = 0.1 },
}

-- Bees: lissajous paths over the flower beds
local BEES = {
	{ y = 0.78, amplitude = 0.4, speed = 0.33, phase = 0, size = 26 },
	{ y = 0.64, amplitude = 0.32, speed = 0.24, phase = 2.1, size = 20 },
}

return function(props: Props): Frame
	local clouds = {}
	local bees = {}

	for index, cloud in CLOUDS do
		local position = Value(UDim2.fromScale(-0.3, cloud.y))
		clouds[index] = position
	end

	for index in BEES do
		bees[index] = {
			position = Value(UDim2.fromScale(0.5, 0.7)),
			rotation = Value(0),
			image = Value(Assets.Scenery.BeeRight),
		}
	end

	local unbind = clock.bind(function(now)
		for index, cloud in CLOUDS do
			-- Travel from off-screen left to off-screen right, then wrap around
			local progress = (now * cloud.speed + cloud.phase) % 1
			clouds[index]:set(UDim2.fromScale(-0.35 + progress * 1.7, cloud.y))
		end

		for index, bee in BEES do
			local t = now * bee.speed + bee.phase
			local x = 0.5 + math.sin(t) * bee.amplitude
			local dx = math.cos(t)
			local y = bee.y + math.sin(t * 2.3) * 0.05 + math.sin(now * 9 + index) * 0.006

			local state = bees[index]
			state.position:set(UDim2.fromScale(x, y))
			state.rotation:set(math.sin(now * 7 + index) * 8)
			state.image:set(if dx >= 0 then Assets.Scenery.BeeRight else Assets.Scenery.BeeLeft)
		end
	end)

	local children = {
		-- Town fills the widget, anchored to the bottom so the houses always show
		Image {
			AnchorPoint = Vector2.new(0.5, 1),
			Position = UDim2.fromScale(0.5, 1),
			Size = Computed(function(use)
				local size = use(props.RootSize)
				local width = math.max(size.X, size.Y * TOWN_ASPECT) + 2

				return UDim2.fromOffset(width, width / TOWN_ASPECT)
			end),
			Image = Assets.Scenery.Town,
			ZIndex = 0,
		},
	}

	for index, cloud in CLOUDS do
		table.insert(
			children,
			Image {
				AnchorPoint = Vector2.new(0.5, 0),
				Position = clouds[index],
				Size = UDim2.fromOffset(cloud.width, cloud.width * 0.55),
				ScaleType = Enum.ScaleType.Fit,
				Image = Assets.Scenery[cloud.image],
				ImageTransparency = cloud.transparency,
				ZIndex = 1,
			}
		)
	end

	for index, bee in BEES do
		local state = bees[index]

		table.insert(
			children,
			Image {
				AnchorPoint = Vector2.new(0.5, 0.5),
				Position = state.position,
				Rotation = state.rotation,
				Size = UDim2.fromOffset(bee.size, bee.size),
				ScaleType = Enum.ScaleType.Fit,
				Image = state.image,
				ZIndex = 2,
			}
		)
	end

	return New "Frame" {
		Name = "Scenery",
		Size = UDim2.fromScale(1, 1),
		BackgroundColor3 = Theme.Colors.Sky,
		BorderSizePixel = 0,
		ClipsDescendants = true,
		ZIndex = 0,
		[Cleanup] = unbind,
		[Children] = children,
	}
end
