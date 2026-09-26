local Aragon = script:FindFirstAncestor("Aragon")

local Fusion = require(Aragon.Packages.Fusion)

local New = Fusion.New
local Hydrate = Fusion.Hydrate

type Props = {
	[any]: any,
}

return function(props: Props): ImageLabel
	return Hydrate(New("ImageLabel") {
		BorderSizePixel = 0,
		BackgroundTransparency = 1,
	})(props)
end
