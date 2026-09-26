--!strict
-- One shared Heartbeat for every ambient animation in the plugin (clouds,
-- bees, floating mascots). It only runs while the Aragon widget is open, so
-- the plugin costs nothing while hidden.

local RunService = game:GetService("RunService")

local plugin = script:FindFirstAncestorWhichIsA("Plugin")

type Callback = (time: number, delta: number) -> ()

local callbacks: { [any]: Callback } = {}
local connection: RBXScriptConnection? = nil
local active = false

local clock = {}

local function update()
	local shouldRun = active and next(callbacks) ~= nil

	if shouldRun and not connection then
		connection = RunService.Heartbeat:Connect(function(delta)
			local now = os.clock()

			for _, callback in callbacks do
				callback(now, delta)
			end
		end)
	elseif not shouldRun and connection then
		connection:Disconnect()
		connection = nil
	end
end

--- Registers an animation callback, returns a function that removes it
function clock.bind(callback: Callback): () -> ()
	local key = {}
	callbacks[key] = callback

	-- Draw one frame right away so nothing starts in a blank position
	callback(os.clock(), 0)
	update()

	return function()
		callbacks[key] = nil
		update()
	end
end

--- The widget tells the clock whether anything is visible
function clock.setActive(isActive: boolean)
	active = isActive
	update()
end

plugin.Unloading:Once(function()
	active = false
	callbacks = {}
	update()
end)

return clock
