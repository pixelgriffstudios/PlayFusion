local options = require "mp.options"

local settings = {
    interval = 25,
}
options.read_options(settings, "jukebox_visualizer")

local modes = {
    {
        name = "RAINBOW FREQUENCY BARS",
        graph = "[aid1]asplit=2[ao][viz];[viz]showcqt=s=960x540:fps=30:count=4:axis=false[vo]",
    },
    {
        name = "SCROLLING SPECTROGRAM",
        graph = "[aid1]asplit=2[ao][viz];[viz]showspectrum=s=960x540:mode=combined:color=rainbow:slide=scroll:scale=cbrt:fps=30[vo]",
    },
    {
        name = "NEON OSCILLOSCOPE",
        graph = "[aid1]asplit=2[ao][viz];[viz]showwaves=s=960x540:mode=cline:colors=00ffcc|ff00ff:r=30:scale=sqrt[vo]",
    },
    {
        name = "STEREO VECTOR SCOPE",
        graph = "[aid1]asplit=2[ao][viz];[viz]avectorscope=s=960x540:mode=lissajous:draw=aaline:scale=cbrt:rc=80:gc=200:bc=255:rf=10:gf=8:bf=5[vo]",
    },
}

local current = 1
local cycle_timer = nil

local function apply_visualizer(show_name)
    mp.set_property("lavfi-complex", modes[current].graph)
    if show_name then
        mp.osd_message("VISUAL: " .. modes[current].name, 2)
    end
end

local function cycle_visualizer()
    current = (current % #modes) + 1
    apply_visualizer(true)
end

mp.add_key_binding(nil, "cycle", cycle_visualizer)

mp.register_event("file-loaded", function()
    apply_visualizer(false)
    if cycle_timer then
        cycle_timer:kill()
    end
    cycle_timer = mp.add_periodic_timer(
        math.max(5, math.min(25, tonumber(settings.interval) or 25)),
        cycle_visualizer
    )
end)
