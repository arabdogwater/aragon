local Aragon = script:FindFirstAncestor("Aragon")
local App = Aragon.App
local Components = App.Components

local Assets = require(App.Assets)
local Theme = require(App.Theme)

local StatusCard = require(Components.StatusCard)
local List = require(Components.List)

-- Playtests run in a separate DataModel, the connector only works in edit mode
return function(): { Instance }
	return {
		List {},
		StatusCard {
			Title = "Napping during playtest",
			Status = "Playtest",
			Body = "Stop the playtest and Aragon picks sync back up in edit mode.",
			Color = Theme.Colors.TextDimmed,
			Mascot = Assets.Mascots.Sleepy,
		},
	}
end
