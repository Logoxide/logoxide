# Audio with timings

An audio Bible that highlights each verse as it's read, and starts reading where the reader
taps. logoxide never stores audio: a recording is listed by its URL, size, and SHA-256, and where
each verse is in it is a WebVTT file, a cue per verse. The app plays the file and asks the
timings which verse is at the current time.

The examples run in order, starting from the helpers in
[Bibles in your app](../apps.md#setting-up) (`lx`, `topos`, `get`). They're made up so they run
anywhere: a timings file written here, and a five-byte "recording" (the word `hello`) standing in
for an MP3. A real recording is the same with its real bytes. The bindings' README has the
methods in full, under "Audio and video"; [doc/manifest.md](../manifest.md#media) specifies
`[media]` and the timings.

- [Timings](#timings)
- [The verse being read](#the-verse-being-read)
- [Starting at a verse](#starting-at-a-verse)
- [Downloading and keeping a recording](#downloading-and-keeping-a-recording)
- [Publishing a recording](#publishing-a-recording)

## Timings

A timings file is WebVTT. Each cue's identifier is the verse it reads, as a USFM reference
(`JHN 3:16`, or `JHN 3:17-18` for verses read together), and its time range is where the verse is
in the recording (`mm:ss.mmm` or `hh:mm:ss.mmm`). Its text may be the verse's or nothing. Cues not
named by a verse (an introduction, music) are skipped. A forced aligner (aeneas, WhisperX, or
any that writes WebVTT) makes one from a recording and its text; name each cue by its verse.

`parseTimings` reads one: each verse's book (its USFM code and topos's number), chapter, verse,
`last` verse (for a bridge), and start and end in milliseconds. A malformed time is an `Invalid`
error naming its line.

```ts
const vtt = `WEBVTT

Introduction
00:00.000 --> 00:03.200
The Gospel of John, chapter 3

JHN 3:16
00:03.200 --> 00:09.500
For God so loved the world …

JHN 3:17-18
00:09.500 --> 00:21.000
For God did not send his Son …
`;
const timings = lx.parseTimings(vtt);
console.log(timings.map((t) => `${t.book} ${t.chapter}:${t.verse}-${t.last} ${t.startMs}-${t.endMs}`)); // → [ 'JHN 3:16-16 3200-9500', 'JHN 3:17-18 9500-21000' ]
console.log(timings[0].bookId); // → 43
```

```python
vtt = """WEBVTT

Introduction
00:00.000 --> 00:03.200
The Gospel of John, chapter 3

JHN 3:16
00:03.200 --> 00:09.500
For God so loved the world …

JHN 3:17-18
00:09.500 --> 00:21.000
For God did not send his Son …
"""
timings = lx.parse_timings(vtt)
print([f"{t.book} {t.chapter}:{t.verse}-{t.last} {t.start_ms}-{t.end_ms}" for t in timings])  # → ['JHN 3:16-16 3200-9500', 'JHN 3:17-18 9500-21000']
print(timings[0].book_id)  # → 43
```

```rust
use logoxide_text::timings::{VerseTiming, parse_timings};

fn timings(vtt: &str) -> Result<Vec<VerseTiming>> {
    let timings = parse_timings(vtt)?;
    for t in &timings {
        println!("{} {}:{}-{} {}-{}", t.book, t.chapter, t.verse, t.last, t.start_ms, t.end_ms); // JHN 3:16-16 3200-9500
    }
    Ok(timings)
}
```

## The verse being read

Timings come in the order they're read, so the verse at a time is found by a binary search; a time
between verses (an introduction, a pause) has none. Ask it each time the player's position
changes (`timeupdate` in a page, a few times a second) and highlight the verse when it changes.

```ts
import { type VerseTiming } from "logoxide";

/** The verse being read at `ms`, if any */
function verseAt(timings: VerseTiming[], ms: number): VerseTiming | null {
  let [low, high] = [0, timings.length - 1];
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (ms < timings[mid].startMs) high = mid - 1;
    else if (ms >= timings[mid].endMs) low = mid + 1;
    else return timings[mid];
  }
  return null;
}

console.log(verseAt(timings, 1000), verseAt(timings, 5000)?.verse, verseAt(timings, 15000)?.last); // → null 16 18
```

```python
import bisect

from logoxide import VerseTiming


def verse_at(timings: list[VerseTiming], ms: int) -> VerseTiming | None:
    """The verse being read at `ms`, if any"""
    i = bisect.bisect_right([t.start_ms for t in timings], ms) - 1
    return timings[i] if i >= 0 and ms < timings[i].end_ms else None


print(verse_at(timings, 1000), verse_at(timings, 5000).verse, verse_at(timings, 15000).last)  # → None 16 18
```

```rust
/// The verse being read at `ms`, if any
fn verse_at(timings: &[VerseTiming], ms: u32) -> Option<&VerseTiming> {
    let i = timings.partition_point(|t| t.start_ms <= ms).checked_sub(1)?;
    (ms < timings[i].end_ms).then(|| &timings[i])
}
```

In a page, with the passage shown as HTML (a book's `html`, whose verse markers carry
`data-osis-id`, or verses you laid out yourself), the highlight is a class moved from verse to
verse:

```ts
// In a page: `audio` is an <audio> element playing the recording
let current: VerseTiming | null = null;
audio.addEventListener("timeupdate", () => {
  const now = verseAt(timings, audio.currentTime * 1000);
  if (now === current) return;
  document.querySelector(".reading")?.classList.remove("reading");
  if (now) document.querySelector(`[data-verse="${now.chapter}:${now.verse}"]`)?.classList.add("reading");
  current = now;
});
```

## Starting at a verse

To read from a verse the reader taps, find its timing (a verse read with others is in a bridge's
`verse` to `last`) and seek to its start. A verse with no timing isn't in this recording.

```ts
/** Where `chapter:verse` starts, in milliseconds, if the recording reads it */
const startOf = (timings: VerseTiming[], chapter: number, verse: number) =>
  timings.find((t) => t.chapter === chapter && t.verse <= verse && verse <= t.last)?.startMs ?? null;

console.log(startOf(timings, 3, 18), startOf(timings, 3, 1)); // → 9500 null
// In a page: audio.currentTime = startOf(timings, 3, 18)! / 1000; audio.play();
```

```python
def start_of(timings: list[VerseTiming], chapter: int, verse: int) -> int | None:
    """Where `chapter:verse` starts, in milliseconds, if the recording reads it"""
    return next((t.start_ms for t in timings if t.chapter == chapter and t.verse <= verse <= t.last), None)


print(start_of(timings, 3, 18), start_of(timings, 3, 1))  # → 9500 None
```

```rust
/// Where `chapter:verse` starts, in milliseconds, if the recording reads it
fn start_of(timings: &[VerseTiming], chapter: u16, verse: u16) -> Option<u32> {
    (timings.iter())
        .find(|t| t.chapter == chapter && (t.verse..=t.last).contains(&verse))
        .map(|t| t.start_ms)
}
```

A recording numbered by another versification than the text shown (a Psalm's title as verse 1, or
not) needs its verses mapped first: the bindings' README, under "Versification maps".

## Downloading and keeping a recording

A recording is a `MediaFile`: its path, URL (and `mirrors`, tried in order after it), size, and
SHA-256. A package lists them (`pkg.media()`); an app with recordings of its own can write the
record itself. Download it when it's first played, check it with `verifyMedia` (an `Integrity`
error says what differs: the size, or the hash), and keep it at `mediaStorePath`, a path by its
hash (`media/2c/2cf2…`) under your app's cache, so two versions listing the same file share one
copy. Since the hash says which bytes are meant, any mirror's copy is as good as the first.

```ts
import { LogoxideErrorException, type MediaFile } from "logoxide";

const recording: MediaFile = {
  path: "JHN.mp3",
  url: "https://example.org/audio/JHN.mp3",
  mirrors: ["https://mirror.example.org/audio/JHN.mp3"],
  size: 5n,
  sha256: "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
  book: "JHN",
  timings: null,
};
const downloaded = new TextEncoder().encode("hello"); // in an app: await get([recording.url, ...recording.mirrors])
lx.verifyMedia(recording, downloaded); // nothing: it's the file listed
try {
  lx.verifyMedia(recording, new TextEncoder().encode("hellO"));
} catch (error) {
  console.log((error as LogoxideErrorException).value.tag); // → Integrity
}
console.log(lx.mediaStorePath(recording)); // → media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

```python
from logoxide import LogoxideErrorException, MediaFile

recording = MediaFile(
    path="JHN.mp3",
    url="https://example.org/audio/JHN.mp3",
    mirrors=["https://mirror.example.org/audio/JHN.mp3"],
    size=5,
    sha256="sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
    book="JHN",
    timings=None,
)
downloaded = b"hello"  # in an app: get([recording.url, *recording.mirrors])
lx.verify_media(recording, downloaded)  # nothing: it's the file listed
try:
    lx.verify_media(recording, b"hellO")
except LogoxideErrorException as error:
    print(type(error.error).__name__, error.error.message)  # → LogoxideErrorIntegrity https://example.org/audio/JHN.mp3: its SHA-256 is …
print(lx.media_store_path(recording))  # → media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

```rust
use logoxide::media::{MediaFile, store_path, verify};

fn keep(downloaded: &[u8]) -> Result<String> {
    // What a package lists (logoxide::media), or an app's own: MediaFile::of makes one from bytes
    let recording = MediaFile::of("JHN.mp3", "https://example.org/audio/JHN.mp3", b"hello");
    verify(&recording, downloaded)?; // a Mismatch error: the size, or the hash
    Ok(store_path(&recording)) // media/2c/2cf24dba…
}
```

In a browser, keep the bytes in Cache Storage or IndexedDB by that path; a native app writes them
under its cache folder. A recording is large, so stream it to the player from the first download
and check it once it's whole, rather than holding it before playing.

## Publishing a recording

A package lists recordings in its `logoxide.toml`'s `[media]`, with a timings folder holding a
`.vtt` for each file (`timings/JHN.vtt` for `JHN.mp3`). A recording of another package's text
names it (`text = "bsb"`, a dependency) and has no text of its own. `lx media add <file> --url
<url>` lists a file with its size and hash, and `lx media check` checks the timings against the
text (every verse exists, in order, without overlapping) and downloads every listed file to check
it. An app opening such a package gets each file (`media()`) and its timings (`timings(file)`)
without reading the `.vtt` itself.

```toml
[media]
text = "bsb"
timings = "timings/"

[[media.files]]
path = "JHN.mp3"
url = "https://example.org/audio/JHN.mp3"
size = 31457280
sha256 = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
book = "JHN"
```
