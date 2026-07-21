import { mkdir, rm } from "node:fs/promises";

const output = "../casefile-server/web";
await rm(output, { force: true, recursive: true });
await mkdir(`${output}/assets`, { recursive: true });

const result = await Bun.build({
  entrypoints: ["src/main.tsx"],
  outdir: `${output}/assets`,
  naming: "app.js",
  target: "browser",
  minify: true,
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
});

if (!result.success) {
  result.logs.forEach((message) => console.error(message));
  process.exit(1);
}

const css = Bun.spawn([
  "bun",
  "run",
  "tailwindcss",
  "-i",
  "src/index.css",
  "-o",
  `${output}/assets/app.css`,
  "--minify",
]);
if ((await css.exited) !== 0) process.exit(1);

const html = (await Bun.file("index.html").text())
  .replace("./src/index.css", "/assets/app.css")
  .replace("./src/main.tsx", "/assets/app.js");
await Bun.write(`${output}/index.html`, html);
