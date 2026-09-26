local plugin = script:FindFirstAncestorWhichIsA("Plugin")

local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components
local Util = Components.Util

local Fusion = require(Aragon.Packages.Fusion)

local stripProps = require(Util.stripProps)

local Hydrate = Fusion.Hydrate

local COMPONENT_ONLY_PROPS = {
	"Name",
}

type Props = {
	Name: string,
	[any]: any,
}

return function(props: Props): PluginToolbar
	return Hydrate(plugin:CreateToolbar(props.Name))(stripProps(props, COMPONENT_ONLY_PROPS))
end
