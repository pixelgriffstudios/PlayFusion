local modes = {
    {
        name = "RAINBOW FREQUENCY BARS",
        graph = "[aid1]asplit=2[ao][viz];[viz]showcqt=s=1280x720:fps=30:count=4:axis=false[vo]",
    },
    {
        name = "SCROLLING SPECTROGRAM",
        graph = "[aid1]asplit=2[ao][viz];[viz]showspectrum=s=1280x720:mode=combined:color=rainbow:slide=scroll:scale=cbrt:fps=30[vo]",
    },
    {
        name = "NEON OSCILLOSCOPE",
        graph = "[aid1]asplit=2[ao][viz];[viz]showwaves=s=1280x720:mode=cline:colors=00ffcc|ff00ff:r=30:scale=sqrt[vo]",
    },
    {
        name = "STEREO VECTOR SCOPE",
        graph = "[aid1]asplit=2[ao][viz];[viz]avectorscope=s=1280x720:mode=lissajous:draw=aaline:scale=cbrt:rc=80:gc=200:bc=255:rf=10:gf=8:bf=5[vo]",
    },
}

local current = 1

local function cycle_visualizer()
    current = (current % #modes) + 1
    mp.set_property("lavfi-complex", modes[current].graph)
    mp.osd_message("VISUAL: " .. modes[current].name, 2)
end

mp.add_key_binding(nil, "cycle", cycle_visualizer)

mp.register_event("file-loaded", function()
    mp.add_timeout(0.5, function()
        mp.osd_message(
            "Y: CHANGE VISUAL  |  A: PAUSE  |  LB/RB: TRACK  |  B: EXIT",
            4
        )
    end)
end)
