--!strict
local RunService = game:GetService("RunService")

local Config = require(script.Config)

Config.load()

-- Playtests: only forward the Output (server or client) so errors reach the dashboard and AI agents
if RunService:IsRunning() then
	require(script.OutputForwarder).start(if RunService:IsServer() then "Server" else "Client")
	return
end

local Log = require(script.Log)
local App = require(script.App)

Log.__setLevel(Config:get("LogLevel"))
App.new()

require(script.OutputForwarder).start("Edit")

Config:onChanged("LogLevel", function(level)
	Log.__setLevel(level)
end)
