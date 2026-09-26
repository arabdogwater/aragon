local plugin = script:FindFirstAncestorWhichIsA("Plugin")

local Aragon = script:FindFirstAncestor("Aragon")

local Util = require(Aragon.Util)
local Log = require(Aragon.Log)

export type Level = "Hub" | "Place" | "Game" | "Global"
export type Setting =
	"InitialSyncPriority"
	| "Host"
	| "Port"
	| "AutoConnect"
	| "AutoReconnect"
	| "LiveHydrate"
	| "KeepUnknowns"
	| "TwoWaySync"
	| "SyncbackProperties"
	| "OnlyCodeMode"
	| "OpenInEditor"
	| "Https"
	| "DiffLinesLimit"
	| "DisplayPrompts"
	| "ChangesThreshold"
	| "LogLevel"
	| "OverridePackages"

type Config = {
	__configs: { [Level]: { [Setting]: any } },
	__callbacks: { [Setting]: { (value: any) -> () } },
	load: () -> (),
	get: (self: Config, setting: Setting, level: Level?, exact: boolean?) -> any,
	getDefault: (self: Config, setting: Setting) -> any,
	set: (self: Config, setting: Setting, value: any, level: Level) -> (),
	restoreDefaults: (self: Config, level: Level) -> (),
	setHub: (self: Config, settings: { [string]: any }) -> (),
	onChanged: (self: Config, setting: Setting, callback: (value: any) -> ()) -> () -> (),
}

local DEFAULTS = {
	InitialSyncPriority = "Client",
	Host = "localhost",
	Port = 7373,
	AutoConnect = true,
	AutoReconnect = false,
	LiveHydrate = true,
	KeepUnknowns = true,
	TwoWaySync = false,
	SyncbackProperties = false,
	OnlyCodeMode = true,
	OpenInEditor = false,
	Https = false,
	DiffLinesLimit = 3000,
	DisplayPrompts = "Always",
	ChangesThreshold = 5,
	LogLevel = "Warn",
	OverridePackages = true,
}

-- `Hub` holds the settings pushed by the Aragon dashboard, it is kept in
-- memory only and always wins over the locally stored levels
local CONFIGS_ORDERED = { "Hub", "Place", "Game", "Global" }
local CONFIGS = {
	Place = "AragonConfigPlace_" .. game.PlaceId,
	Game = "AragonConfigGame_" .. game.GameId,
	Global = "AragonConfigGlobal",
}

local Config: Config = {
	__configs = { Hub = {} },
	__callbacks = {},
} :: Config

function Config.load()
	for level, config in CONFIGS do
		config = plugin:GetSetting(config)

		if config and type(config) == "table" then
			Config.__configs[level] = config
		else
			Config.__configs[level] = {}
		end
	end
end

function Config:get(setting, level, exact)
	local default = DEFAULTS[setting]
	local relevant = not level

	if default == nil then
		Log.error(`Setting '{setting}' does not exist!`)
	end

	if exact then
		return self.__configs[level][setting]
	end

	if level and level ~= "Global" then
		local index = table.find(CONFIGS_ORDERED, level)

		if not index then
			Log.error(`Config level '{level}' does not exist!`)
		end

		level = CONFIGS_ORDERED[index + 1]
	end

	for _, config in CONFIGS_ORDERED do
		if not relevant then
			if config == level then
				relevant = true
			else
				continue
			end
		end

		local value = self.__configs[config][setting]

		if value ~= nil then
			return value
		end
	end

	return default
end

function Config:setHub(settings)
	local previous = self.__configs.Hub
	local current = {}

	for setting, value in settings do
		local default = DEFAULTS[setting]

		-- Connection settings always stay local
		if default == nil or setting == "Host" or setting == "Port" or setting == "Https" then
			continue
		end

		current[setting] = Util.cast(value, type(default))
	end

	self.__configs.Hub = current

	for setting in DEFAULTS do
		if previous[setting] ~= current[setting] and self.__callbacks[setting] then
			for _, callback in self.__callbacks[setting] do
				callback(self:get(setting))
			end
		end
	end
end

function Config:getDefault(setting)
	local default = DEFAULTS[setting]

	if default == nil then
		Log.error(`Setting '{setting}' does not exist!`)
	end

	return default
end

function Config:set(setting, value, level)
	local default = DEFAULTS[setting]
	local config = self.__configs[level]

	if default == nil then
		Log.error(`Setting '{setting}' does not exist!`)
	end

	if value == config[setting] then
		return
	end

	value = Util.cast(value, type(default))
	config[setting] = if value == default then nil else value

	if self.__callbacks[setting] then
		for _, callback in self.__callbacks[setting] do
			callback(self:get(setting))
		end
	end

	if not next(config) then
		plugin:SetSetting(CONFIGS[level], nil)
	else
		plugin:SetSetting(CONFIGS[level], config)
	end
end

function Config:restoreDefaults(level)
	self.__configs[level] = {}

	for setting, _ in self.__configs[level] do
		if self.__callbacks[setting] then
			for _, callback in self.__callbacks[setting] do
				callback(self:getDefault(setting))
			end
		end
	end

	plugin:SetSetting(CONFIGS[level], nil)
end

function Config:onChanged(setting, callback)
	if not self.__callbacks[setting] then
		self.__callbacks[setting] = {}
	end

	local id = Util.generateGUID()
	self.__callbacks[setting][id] = callback

	return function()
		self.__callbacks[setting][id] = nil
	end
end

return Config
