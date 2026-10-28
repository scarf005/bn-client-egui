// Turns a demo directory recorded by `crates/egui/tests/it/demo_editor_video.rs` into an mp4: the frames
// (`frames/NNNNN.png`) each shown for the milliseconds of `timing.tsv`, the captions of `captions.tsv` burned in
// on a strip under the picture, so that no caption covers what the window shows.
//
// deno run --allow-read --allow-write --allow-run=ffmpeg tools/demo_video.ts <demo dir> [out.mp4]
import { basename, dirname, join, resolve } from "jsr:@std/path@1";

const FPS = 30;
const FONT = "Noto Sans CJK KR";
const STRIP = 90;

/// The rows of a tab-separated file as `[number, text]`.
const rows = async (path: string) =>
  (await Deno.readTextFile(path)).split("\n").filter((l) => l.trim()).map(
    (l) => {
      const [n, ...rest] = l.split("\t");
      return [Number(n), rest.join("\t")] as const;
    },
  );

const srtTime = (ms: number) => {
  const pad = (n: number, w = 2) => String(Math.floor(n)).padStart(w, "0");
  return `${pad(ms / 3600000)}:${pad((ms / 60000) % 60)}:${
    pad((ms / 1000) % 60)
  },${pad(ms % 1000, 3)}`;
};

const [dirArg, outArg] = Deno.args;
if (!dirArg) {
  console.error("usage: demo_video.ts <demo dir> [out.mp4]");
  Deno.exit(2);
}
const dir = resolve(dirArg);
const out = resolve(outArg ?? join(dirname(dir), `${basename(dir)}.mp4`));

const timing = await rows(join(dir, "timing.tsv"));
const captions = await rows(join(dir, "captions.tsv"));

// The millisecond each frame starts at.
const starts = new Map<number, number>();
let total = 0;
for (const [frame, ms] of timing) {
  starts.set(frame, total);
  total += Number(ms);
}

// The concat demuxer repeats the last file so its duration holds.
const names = timing.map(([f]) => `frames/${String(f).padStart(5, "0")}.png`);
const list = timing.map(([, ms], i) =>
  `file '${names[i]}'\nduration ${(Number(ms) / 1000).toFixed(3)}`
);
list.push(`file '${names.at(-1)}'`);
await Deno.writeTextFile(join(dir, "frames.ffconcat"), list.join("\n") + "\n");

const srt = captions.map(([frame, text], i) => {
  const from = starts.get(frame) ?? total;
  const to = i + 1 < captions.length
    ? starts.get(captions[i + 1][0]) ?? total
    : total;
  return `${i + 1}\n${srtTime(from)} --> ${srtTime(to)}\n${text}\n`;
}).join("\n");
await Deno.writeTextFile(join(dir, "captions.srt"), srt);

const style =
  `FontName=${FONT},FontSize=7,Bold=1,PrimaryColour=&H00FFFFFF,OutlineColour=&HC0000000,BorderStyle=3,Outline=1,Shadow=0,MarginV=5`;
const filter =
  `fps=${FPS},pad=iw:ih+${STRIP}:0:0:black,subtitles=captions.srt:force_style='${style}',format=yuv420p`;
const { code } = await new Deno.Command("ffmpeg", {
  cwd: dir,
  args: [
    "-y",
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "concat",
    "-safe",
    "0",
    "-i",
    "frames.ffconcat",
    "-vf",
    filter,
    "-c:v",
    "libx264",
    "-crf",
    "20",
    "-preset",
    "medium",
    "-pix_fmt",
    "yuv420p",
    "-movflags",
    "+faststart",
    out,
  ],
  stdout: "inherit",
  stderr: "inherit",
}).output();
if (code !== 0) Deno.exit(code);
console.log(`${out} (${(total / 1000).toFixed(1)} s)`);
