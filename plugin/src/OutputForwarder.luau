--!strict
-- Sends Studio's Output window to the Aragon hub. The dashboard shows it with errors linked to the files that
-- raised them, and the hub mirrors it to <project>/.aragon/studio-output.log so an AI agent can read runtime
-- errors. Runs in edit mode and in playtests (the server and the client each forward their own output).

local LogService = game:GetService("LogService")

local Aragon = script:FindFirstAncestor("Aragon")

local Http = require(Aragon.Client.Http)
local Config = require(Aragon.Config)
local Place = require(Aragon.Place)

local FLUSH_EVERY = 0.4
local MAX_BUFFER = 400
local MAX_TEXT = 4000

local KINDS = {
	[Enum.MessageType.MessageOutput] = "output",
	[Enum.MessageType.MessageInfo] = "info",
	[Enum.MessageType.MessageWarning] = "warn",
	[Enum.MessageType.MessageError] = "error",
}

type Message = { t: number, kind: string, text: string }

local OutputForwarder = {}

local started = false

function OutputForwarder.start(context: "Edit" | "Server" | "Client")
	if started then
		return
	end

	started = true

	local buffer: { Message } = {}
	local sending = false
	local failures = 0

	LogService.MessageOut:Connect(function(message: string, messageType: Enum.MessageType)
		if #buffer >= MAX_BUFFER then
			table.remove(buffer, 1)
		end

		table.insert(buffer, {
			t = DateTime.now().UnixTimestampMillis / 1000,
			kind = KINDS[messageType] or "output",
			text = if #message > MAX_TEXT then message:sub(1, MAX_TEXT) .. "…" else message,
		})
	end)

	task.spawn(function()
		while true do
			-- Back off while the hub isn't running
			task.wait(FLUSH_EVERY + math.min(failures, 10) * 1.5)

			if #buffer == 0 or sending then
				continue
			end

			local batch = buffer
			buffer = {}
			sending = true

			local protocol = if Config:get("Https") then "https" else "http"
			local url = `{protocol}://{Config:get("Host")}:{Config:get("Port")}/hub/output`

			Http.post(url, {
				placeId = game.PlaceId,
				placeName = Place.Name,
				context = context,
				messages = batch,
			})
				:andThen(function()
					failures = 0
				end)
				:catch(function()
					-- Never log here: the warning would be forwarded too and loop
					failures += 1
				end)
				:finally(function()
					sending = false
				end)
		end
	end)
end

return OutputForwarder
