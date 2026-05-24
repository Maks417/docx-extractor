import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { chmod, mkdir, readFile, rename, stat, unlink } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const REPO = "Maks417/docx-extractor";

interface PlatformAsset {
  asset: string;
  binaryName: string;
}

function detectAsset(): PlatformAsset {
  const { platform, arch } = process;
  if (platform === "linux" && arch === "x64") {
    return { asset: "docx-extractor-linux-x86_64", binaryName: "docx-extractor" };
  }
  if (platform === "darwin" && arch === "arm64") {
    return { asset: "docx-extractor-macos-aarch64", binaryName: "docx-extractor" };
  }
  if (platform === "darwin" && arch === "x64") {
    return { asset: "docx-extractor-macos-x86_64", binaryName: "docx-extractor" };
  }
  if (platform === "win32" && arch === "x64") {
    return {
      asset: "docx-extractor-windows-x86_64.exe",
      binaryName: "docx-extractor.exe",
    };
  }
  throw new Error(
    `Unsupported platform: ${platform}/${arch}. ` +
      "docx-extractor publishes prebuilt binaries for linux-x64, macos-x64, macos-arm64, and windows-x64. " +
      "For other targets, build from source: https://github.com/Maks417/docx-extractor",
  );
}

function cacheDir(version: string): string {
  return join(homedir(), ".cache", "docx-extractor-mcp", version);
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function sha256OfFile(path: string): Promise<string> {
  const buf = await readFile(path);
  return createHash("sha256").update(buf).digest("hex");
}

async function downloadTo(url: string, destPath: string): Promise<void> {
  await mkdir(dirname(destPath), { recursive: true });
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok || !res.body) {
    throw new Error(`Failed to download ${url}: HTTP ${res.status} ${res.statusText}`);
  }
  const tmp = `${destPath}.tmp-${process.pid}`;
  await pipeline(Readable.fromWeb(res.body as never), createWriteStream(tmp));
  await rename(tmp, destPath);
}

async function fetchText(url: string): Promise<string> {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`Failed to fetch ${url}: HTTP ${res.status} ${res.statusText}`);
  }
  return res.text();
}

function expectedHashFromSums(sums: string, assetName: string): string | null {
  for (const line of sums.split(/\r?\n/)) {
    const m = line.match(/^([0-9a-f]{64})\s+\*?(.+)$/i);
    if (m && m[2].trim() === assetName) return m[1].toLowerCase();
  }
  return null;
}

let packageVersionCache: string | null = null;

async function packageVersion(): Promise<string> {
  if (packageVersionCache) return packageVersionCache;
  const here = dirname(fileURLToPath(import.meta.url));
  const pkgPath = join(here, "..", "package.json");
  const raw = await readFile(pkgPath, "utf8");
  const pkg = JSON.parse(raw) as { version?: string };
  if (!pkg.version) throw new Error("package.json is missing a version field");
  packageVersionCache = pkg.version;
  return pkg.version;
}

export async function ensureBinary(): Promise<string> {
  const version = await packageVersion();
  const { asset, binaryName } = detectAsset();
  const dir = cacheDir(version);
  const binPath = join(dir, binaryName);

  if (await fileExists(binPath)) return binPath;

  const base = `https://github.com/${REPO}/releases/download/v${version}`;
  const assetUrl = `${base}/${asset}`;
  const sumsUrl = `${base}/SHA256SUMS.txt`;

  let expected: string | null = null;
  try {
    const sums = await fetchText(sumsUrl);
    expected = expectedHashFromSums(sums, asset);
  } catch (err) {
    // SHA256SUMS.txt is optional — keep downloading, just skip verification.
    process.stderr.write(
      `docx-extractor-mcp: warning: could not fetch SHA256SUMS.txt (${
        (err as Error).message
      }). Proceeding without checksum verification.\n`,
    );
  }

  await downloadTo(assetUrl, binPath);

  if (expected) {
    const actual = await sha256OfFile(binPath);
    if (actual !== expected) {
      await unlink(binPath).catch(() => {});
      throw new Error(
        `SHA256 mismatch for ${asset}: expected ${expected}, got ${actual}. ` +
          "The download may be corrupt or tampered with — aborting.",
      );
    }
  }

  if (process.platform !== "win32") {
    await chmod(binPath, 0o755);
  }
  return binPath;
}
