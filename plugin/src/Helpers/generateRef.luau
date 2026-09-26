--!strict
local Aragon = script:FindFirstAncestor("Aragon")

local Types = require(Aragon.Types)

return function(): Types.Ref
	local buf = buffer.create(16)

	for i = 0, 15 do
		buffer.writeu8(buf, i, math.random(0, 255))
	end

	return buf
end
