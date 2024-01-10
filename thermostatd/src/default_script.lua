local cool_range, heat_range

local DHO = 0.0 -- Daytime Heat Offset

function init(state)
    -- Example mqtt init call:
    --     state.mqtt:subscribe('home/thermostat/hvac/pinstate')
    -- Later in a tick or evaluate:
    --     local pinstate = state.mqtt["home/thermostat/hvac/pinstate"]
end

function evaluate(state)
    local temp = state.probes.primary
    if state.mode == 'cool' then
        return state:timed_program {
            ['23:00'] = cool_range(temp, 22.0, 22.5),
            ['05:00'] = cool_range(temp, 22.75, 23.75),
        }
    elseif state.mode == 'heat' then
        return state:timed_program {
            ['21:00'] = heat_range(temp, 18.5, 19.5),
            ['07:00'] = heat_range(temp, 20.5, 21.5),
            ['08:30'] = heat_range(temp, 20.0 + DHO, 21.0 + DHO),
        }
    end
end

function tick(state)
    
end

function cool_range(temp, min, max)
    return function()
        if temp < min then
            return 'off'
        elseif temp > max then
            return 'cool'
        end
    end
end

function heat_range(temp, min, max)
    return function()
        if temp < min then
            return 'heat'
        elseif temp > max then
            return 'off'
        end
    end
end
