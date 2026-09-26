local plugin = script:FindFirstAncestorWhichIsA("Plugin")
local Studio = settings().Studio

local Aragon = script:FindFirstAncestor("Aragon")

local Fusion = require(Aragon.Packages.Fusion)

local Value = Fusion.Value

-- Aragon always wears its own sunny cartoon look (same as the dashboard),
-- no matter the Studio theme: cream paper, espresso ink, shiba gold
local COLORS = {
	Brand = Color3.fromRGB(255, 194, 26),
	BrandHover = Color3.fromRGB(255, 208, 74),
	BrandPress = Color3.fromRGB(242, 176, 0),
	Ok = Color3.fromRGB(63, 208, 125),
	Warn = Color3.fromRGB(255, 194, 26),
	Error = Color3.fromRGB(255, 107, 94),

	Sky = Color3.fromRGB(155, 219, 247),
	Background = Color3.fromRGB(255, 251, 240),
	ActiveBackground = Color3.fromRGB(255, 244, 216),
	Border = Color3.fromRGB(43, 29, 14),
	Shadow = Color3.fromRGB(43, 29, 14),

	Text = Color3.fromRGB(43, 29, 14),
	TextDimmed = Color3.fromRGB(90, 70, 49),
	TextBranded = Color3.fromRGB(43, 29, 14),

	Diff = {
		Add = Color3.fromRGB(35, 138, 78),
		Update = Color3.fromRGB(47, 111, 208),
		Remove = Color3.fromRGB(198, 58, 48),
	},
}

local function init(color: Color3): Fusion.Value<Color3>
	return Value(color)
end

local Theme = {
	Colors = {
		Brand = init(COLORS.Brand),
		BrandHover = init(COLORS.BrandHover),
		BrandPress = init(COLORS.BrandPress),
		Ok = init(COLORS.Ok),
		Warn = init(COLORS.Warn),
		Error = init(COLORS.Error),

		Sky = init(COLORS.Sky),
		Background = init(COLORS.Background),
		ActiveBackground = init(COLORS.ActiveBackground),
		Border = init(COLORS.Border),
		Shadow = init(COLORS.Shadow),

		Text = init(COLORS.Text),
		TextDimmed = init(COLORS.TextDimmed),
		TextBranded = init(COLORS.TextBranded),

		Diff = COLORS.Diff,
	},

	Fonts = {
		Regular = Font.new("rbxasset://fonts/families/Nunito.json", Enum.FontWeight.SemiBold),
		Bold = Font.new("rbxasset://fonts/families/Nunito.json", Enum.FontWeight.ExtraBold),
		Display = Font.fromEnum(Enum.Font.FredokaOne),
		Italic = Font.new("rbxasset://fonts/families/Nunito.json", Enum.FontWeight.SemiBold, Enum.FontStyle.Italic),
		Mono = Font.fromName("RobotoMono"),
		MonoBold = Font.fromName("RobotoMono", Enum.FontWeight.Bold),

		-- required for TextService:GetTextSize()
		Enums = {
			Regular = Enum.Font.Nunito,
			Bold = Enum.Font.Nunito,
			Display = Enum.Font.FredokaOne,
			Italic = Enum.Font.Nunito,
			Mono = Enum.Font.RobotoMono,
			MonoBold = Enum.Font.RobotoMono,
		},
	},

	BorderThickness = 2,
	CornerRadius = 12,
	ListSpacing = 12,
	WidgetPadding = 14,
	Padding = 10,
	ShadowOffset = 4,

	TextSize = {
		Large = 18,
		Medium = 16,
		Small = 14,
	},

	CompSizeY = {
		Large = 36,
		Medium = 30,
		Small = 28,
	},

	SpringSpeed = 30,
	SpringFastSpeed = 60,
	SpringDamping = 1.5,

	-- Studio theme is still tracked for the few widgets that care (diff view)
	IsDark = Value(true),
}

do
	local function updateTheme()
		local _, _, v = Studio.Theme:GetColor(Enum.StudioStyleGuideColor.MainBackground):ToHSV()
		Theme.IsDark:set(v <= 0.6)
	end

	updateTheme()

	local connection = Studio.ThemeChanged:Connect(updateTheme)

	plugin.Unloading:Once(function()
		connection:Disconnect()
		connection = nil
	end)
end

return Theme
