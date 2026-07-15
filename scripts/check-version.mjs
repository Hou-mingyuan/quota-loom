import { readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const packageJson = JSON.parse(
  await readFile(new URL("package.json", root), "utf8"),
);
const tauriConfig = JSON.parse(
  await readFile(new URL("src-tauri/tauri.conf.json", root), "utf8"),
);
const cargoToml = await readFile(new URL("src-tauri/Cargo.toml", root), "utf8");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const versions = new Map([
  ["package.json", packageJson.version],
  ["src-tauri/Cargo.toml", cargoVersion],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
]);
const expected = packageJson.version;
const mismatches = [...versions].filter(([, version]) => version !== expected);

if (mismatches.length > 0) {
  for (const [file, version] of mismatches) {
    console.error(
      `${file} has version ${version ?? "<missing>"}; expected ${expected}`,
    );
  }
  process.exit(1);
}

if (process.env.GITHUB_REF_TYPE === "tag") {
  const expectedTag = `v${expected}`;
  if (process.env.GITHUB_REF_NAME !== expectedTag) {
    console.error(
      `Release tag ${process.env.GITHUB_REF_NAME} does not match ${expectedTag}`,
    );
    process.exit(1);
  }
}

console.log(
  `Version ${expected} is consistent across package and Tauri manifests.`,
);
