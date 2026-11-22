-- Minimal WezTerm config for scripts/screenshots.sh: one window that runs tbloom with the
-- arguments in $TBLOOM_ARGS. Settings come from the environment, so the script is the only
-- place that knows the defaults.
local wezterm = require "wezterm"

local bin = assert(os.getenv "TBLOOM_BIN", "TBLOOM_BIN is not set")
local args = { bin }
for a in (os.getenv "TBLOOM_ARGS" or ""):gmatch "%S+" do
  args[#args + 1] = a
end

local config = wezterm.config_builder()
config.default_prog = args
config.font = wezterm.font_with_fallback { os.getenv "TBLOOM_FONT" or "FiraCode Nerd Font" }
config.font_size = tonumber(os.getenv "TBLOOM_FONT_SIZE" or 13)
config.initial_cols = tonumber(os.getenv "TBLOOM_COLS" or 110)
config.initial_rows = tonumber(os.getenv "TBLOOM_ROWS" or 36)
config.enable_wayland = false -- XWayland: KDE draws the frame and title bar
config.window_decorations = "TITLE | RESIZE"
config.window_padding = { left = 0, right = 0, top = 0, bottom = 0 }
config.hide_tab_bar_if_only_one_tab = true
config.enable_scroll_bar = false
config.check_for_updates = false
config.colors = { background = "#000000" } -- the same black as the scene around the bouquet
config.window_close_confirmation = "NeverPrompt"

wezterm.on("format-window-title", function()
  return os.getenv "TBLOOM_TITLE" or "tbloom"
end)

return config
