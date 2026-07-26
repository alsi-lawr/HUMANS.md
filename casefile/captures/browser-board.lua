--[[
# viset
version = 1
output = "browser-workbench.png"
browser_arguments = ["--hide-scrollbars"]

[devices.desktop]
mobile = false
touch = false
device_scale = 1.0

[devices.desktop.viewport]
width = 2560
height = 1080
]]

local casefile = assert(os.getenv("CASEFILE_BIN"), "CASEFILE_BIN is required")
local root = assert(os.getenv("CASEFILE_ROOT"), "CASEFILE_ROOT is required")
local index = assert(os.getenv("CASEFILE_INDEX"), "CASEFILE_INDEX is required")
local port = "41750"
local url = "http://127.0.0.1:" .. port .. "/"
local server = viset.process.start({
  file = casefile,
  arguments = { "--root", root, "serve", "--port", port, "--index", index },
})

local succeeded, failure = pcall(function()
  viset.http.wait({ url = url, timeout = "20s" })
  viset.page.navigate(url)
  viset.page.wait_for("!document.body.innerText.includes('Refreshing Casefile index')", "20s")
  local function click(label)
    local script = "[...document.querySelectorAll('button')].find(item => item.textContent.includes(" .. string.format("%q", label) .. ")).click()"
    viset.page.evaluate(viset.javascript(script))
  end
  click("demo")
  viset.page.wait_for("document.body.innerText.includes('Investigations')", "10s")
  click("sample")
  viset.page.wait_for("document.body.innerText.includes('Governed work')", "10s")
  click("Boards")
  viset.page.wait_for("document.body.innerText.includes('Delivery boards')", "10s")
  viset.page.wait_for("document.body.innerText.includes('In progress')", "10s")
  viset.snapshot()
end)
viset.process.stop(server)
if not succeeded then error(failure, 0) end
