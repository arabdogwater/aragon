local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Config = require(Aragon.Config)
local Assets = require(App.Assets)
local Theme = require(App.Theme)

local StatusCard = require(Components.StatusCard)
local List = require(Components.List)

type Props = {
	App: { [string]: any },
}

-- Hub not reachable: tell the user to open Aragon on their PC, we keep knocking
return function(_props: Props): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Looking for Aragon",
			Status = "Searching",
			Mascot = Assets.Mascots.Sleepy,
			Body = `Open <b>Aragon</b> on your PC and this place connects on its own.\n<font transparency="0.35">localhost:{Config:get("Port")}</font>`,
			Color = Theme.Colors.Brand,
			Pulse = true,
		},
	}
end
