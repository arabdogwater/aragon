--!strict
-- Talks to the Aragon hub running on the PC: introduces this Studio session
-- (place, game, user) and keeps a long-poll control channel open so the
-- dashboard can push settings, reconnects and messages.

local StudioService = game:GetService("StudioService")

local Aragon = script:FindFirstAncestor("Aragon")

local Promise = require(Aragon.Packages.Promise)

local Http = require(Aragon.Client.Http)
local Place = require(Aragon.Place)
local Log = require(Aragon.Log)
local manifest = require(Aragon.manifest)

export type HelloResponse = {
	state: "Mapped" | "Onboarding" | "Paused" | "Error",
	placeKey: string,
	base: string,
	message: string?,
	projectName: string?,
	settings: { [string]: any },
	initialSyncPriority: string?,
	hubVersion: string,
}

export type Control = {
	kind: "Settings" | "Reconnect" | "Disconnect" | "Toast",
	settings: { [string]: any }?,
	message: string?,
}

local function getStudioUserId(): number?
	local ok, userId = pcall(function()
		return StudioService:GetUserId()
	end)

	return if ok and type(userId) == "number" and userId > 0 then userId else nil
end

local Connector = {}
Connector.__index = Connector

function Connector.new(host: string, port: number, https: boolean)
	local self = setmetatable({}, Connector)

	self.host = host
	self.port = port
	self.https = https
	self.clientId = math.floor(math.random() * 10 ^ 9)

	return self
end

function Connector:getUrl(endpoint: string): string
	local protocol = self.https and "https" or "http"
	return `{protocol}://{self.host}:{self.port}/{endpoint}`
end

function Connector:hello(): Promise.TypedPromise<HelloResponse>
	Log.trace("Connector: hello")

	return Http.post(self:getUrl("hub/hello"), {
		clientId = self.clientId,
		placeId = game.PlaceId,
		gameId = game.GameId,
		placeName = Place.Name,
		isFile = Place.IsFile,
		creatorId = game.CreatorId,
		creatorType = tostring(game.CreatorType):match("%w+$"),
		studioUserId = getStudioUserId(),
		pluginVersion = manifest.package.version,
	}):andThen(function(response)
		return response:decode()
	end)
end

function Connector:poll(): Promise.TypedPromise<{ Control }>
	return Http.post(self:getUrl("hub/poll"), {
		clientId = self.clientId,
	}):andThen(function(response)
		return response:decode()
	end)
end

function Connector:bye()
	Http.post(self:getUrl("hub/bye"), {
		clientId = self.clientId,
	}):catch(function() end)
end

function Connector:synced(placeKey: string)
	Http.post(self:getUrl("hub/synced"), {
		placeKey = placeKey,
	}):catch(function() end)
end

--- Brings the dashboard window to the front
function Connector:focusDashboard()
	Http.post(self:getUrl("api/focus")):catch(function(err)
		Log.debug("Failed to focus the dashboard:", err)
	end)
end

return Connector
