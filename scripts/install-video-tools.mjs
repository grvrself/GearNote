import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { mkdir, rename, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const target = resolve(process.argv[2] ?? '.');
await mkdir(target, { recursive: true });

async function download(name, url, expectedHash) {
  const path = join(target, name);
  const temporary = `${path}.part`;
  const existing = await stat(temporary).then(info => info.size).catch(() => 0);
  const response = await fetch(url, existing ? { headers: { Range: `bytes=${existing}-` } } : undefined);
  if (!response.ok || !response.body) throw new Error(`${name}: HTTP ${response.status}`);
  if (existing && response.status !== 206) throw new Error(`${name}: 服务器不支持续传`);
  await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary, { flags: existing ? 'a' : 'w' }));
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(temporary)) digest.update(chunk);
  const hash = digest.digest('hex');
  if (hash !== expectedHash) throw new Error(`${name}: SHA-256 不匹配 (${hash})`);
  await rename(temporary, path);
  process.stdout.write(`${name}: SHA-256 verified\n`);
}

await download('ffmpeg-release-essentials.zip',
  'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip',
  '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba');
await download('yt-dlp.exe',
  'https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp.exe',
  'f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d');
