import { spawnSync } from 'node:child_process';
import { stat } from 'node:fs/promises';
import { resolveInput } from '../src/storyboard/fetch.js';

const [url, rawSeconds, output] = process.argv.slice(2);
const seconds = Number(rawSeconds);
if (!url || !output || !Number.isInteger(seconds) || seconds < 0 || seconds > 864000) {
  console.error('无效的取帧参数');
  process.exit(2);
}

try {
  let resolved;
  try {
    resolved = await resolveInput(url, { scanHeight: 720, reuseCached: true });
  } catch (error) {
    if (!String(error?.message).includes('HTTP 567')) throw error;
    resolved = await resolveInput(url, { scanHeight: 360, reuseCached: true });
  }
  const ffmpeg = process.env.KNOWFLOW_FFMPEG || 'ffmpeg';
  const result = spawnSync(ffmpeg, [
    '-hide_banner', '-loglevel', 'error', '-nostdin', '-ss', String(seconds),
    '-i', resolved.path, '-frames:v', '1', '-q:v', '3', '-y', output,
  ], { encoding: 'utf8', timeout: 120000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr?.trim() || 'FFmpeg 取帧失败');
  const image = await stat(output);
  if (!image.isFile() || image.size === 0) throw new Error('指定时间点没有可用画面');
} catch (error) {
  console.error(error?.message || String(error));
  process.exit(1);
}
