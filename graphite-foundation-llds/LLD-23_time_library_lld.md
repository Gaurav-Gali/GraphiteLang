# Graphite LLD: `grf.time` Standard Library

**Status:** Design decisions locked for V1 implementation (phased)  
**Target:** Graphite Lang (`.grf`)  
**Library import:** `import grf.time;`  
**Namespace:** `time`  
**Implementation target:** Rust  
**Version scope:** Full API designed; implement in phases (see §18)

---

## 1. Purpose

The `grf.time` library provides current-time access, timestamps, date and clock helpers, formatting, parsing, calendar arithmetic, durations, wall-clock elapsed helpers, timezone formatting, and asynchronous sleep. Monotonic benchmarking helpers are deferred (see §11).

The public API deliberately does **not** introduce Graphite-visible types such as `date`, `clockTime`, `instant`, `duration`, or `timer`. It uses existing Graphite types:

- `str` for readable dates, times, timestamps, and formatted output.
- `int` for integer Unix timestamps and integer-valued time measurements.
- `flt` for fractional Unix timestamps and fractional conversions.
- `bln` for predicates.
- `async<void>` for asynchronous sleep, using Graphite's existing async/await model.

Rust may use internal date/time structures to implement the library safely. Those implementation types are not exposed in Graphite source.

## 2. Design principles

1. **No new public temporal types in V1.** Date/time values are represented by strings and timestamps by numbers.
2. **Predictable string formats.** Date-taking calendar helpers use ISO date strings (`yyyy-MM-dd`).
3. **Explicit conversion.** Timestamp APIs clearly use milliseconds since the Unix epoch.
4. **Calendar arithmetic is not elapsed-time arithmetic.** Adding one calendar day and adding 24 hours are distinct operations.
5. **No fragile string arithmetic.** Functions parse inputs into internal Rust temporal values, perform validated operations, then return strings/numbers.
6. **Consistent errors.** Invalid dates, malformed timestamps, invalid format patterns, overflow, and invalid durations use Graphite's existing runtime error mechanism.
7. **Minimal runtime overhead.** Use wall-clock APIs for current time; use Tokio timers for asynchronous sleep. Monotonic benchmark APIs are deferred until a safe public representation exists.

## 3. Import and namespace

```grf
import grf.time;

str today = time.today();
str timestamp = time.iso();
```

The reserved `grf` domain supplies the standard library. After importing `grf.time`, calls use `time.*`, not `grf.time.*`.

Member names may be Graphite keywords after `.` (for example `time.int()`, `time.flt()`, `time.str()`). The Rust implementations use non-keyword paths such as `current_int` / `current_flt` / `current_str`.

## 4. Current time and timestamp APIs

### 4.1 Current values

```grf
flt timestamp = time.flt();
int timestampMs = time.int();
str readable = time.str();
str iso = time.iso();
str today = time.today();
str now = time.now();
```

| Function | Return type | Contract |
|---|---|---|
| `time.flt()` | `flt` | Current Unix timestamp in milliseconds, including fractional precision where available. |
| `time.int()` | `int` | Current Unix timestamp in integer milliseconds. |
| `time.str()` | `str` | Human-readable current date and time. |
| `time.iso()` | `str` | Current timestamp formatted as ISO 8601. |
| `time.today()` | `str` | Current calendar date formatted as `yyyy-MM-dd`. |
| `time.now()` | `str` | Current clock time formatted as `HH:mm:ss`. |

`time.flt()` remains part of the API. Example elapsed wall-clock measurement:

```grf
flt start = time.flt();
// Work to measure.
flt end = time.flt();
flt elapsedMs = end - start;
```

Wall-clock time can jump due to clock corrections. Monotonic benchmark helpers are deferred (Section 11); do not treat `flt()` / `int()` as high-precision timers.

### 4.2 Epoch and units

- Unix epoch: `1970-01-01T00:00:00Z`.
- `time.int()` and `time.flt()` use **milliseconds**, not seconds.
- `time.flt()` may include fractional milliseconds; its precision depends on the underlying clock and platform.
- Negative timestamps represent instants before the Unix epoch, where supported by the platform implementation.

### 4.3 Timezone policy (locked)

- `time.iso()` returns an ISO 8601 timestamp in UTC, with a `Z` suffix.
- `time.int()` and `time.flt()` are absolute Unix epoch timestamps (milliseconds) and are timezone-independent.
- `time.today()`, `time.now()`, and `time.str()` use the **system-local timezone**.
- `time.formatIn(...)` formats an absolute timestamp in the explicitly requested IANA timezone.
- `time.timezone()` returns the system timezone identifier when it can be determined. If unavailable, it raises a **runtime error** — no silent fallback string.

This policy means `time.today()` can differ from the UTC calendar date near midnight. Documentation must state that it is system-local.

## 5. Date strings and calendar helpers

### 5.1 Canonical date representation

Functions that accept a date string expect the ISO form:

```text
yyyy-MM-dd
```

Example: `"2026-10-26"`.

```grf
str today = time.today();
int year = time.year(today);
int month = time.month(today);
int day = time.day(today);
int weekday = time.dayOfWeek(today);
int doy = time.dayOfYear(today);
int dim = time.daysInMonth(today);
```

| Function | Return type | Contract |
|---|---|---|
| `time.year(dateText)` | `int` | Four-digit calendar year for the parsed date. |
| `time.month(dateText)` | `int` | Month number, `1` through `12`. |
| `time.day(dateText)` | `int` | Day of month. |
| `time.dayOfWeek(dateText)` | `int` | ISO weekday: Monday `1` through Sunday `7`. |
| `time.dayOfYear(dateText)` | `int` | Day of year, `1` through `365` or `366`. |
| `time.daysInMonth(dateText)` | `int` | Number of days in the date's month. |
| `time.isLeapYear(year)` | `bln` | Whether the supplied year is a leap year. |
| `time.isFirstDayOfMonth(dateText)` | `bln` | Whether the date is the first day of its month. |
| `time.isLastDayOfMonth(dateText)` | `bln` | Whether the date is the last day of its month. |
| `time.isToday(dateText)` | `bln` | Whether the date matches the current system-local calendar date. |

### 5.2 Weekday predicates and names

```grf
str today = time.today();

bln weekday = time.isWeekday(today);
bln weekend = time.isWeekend(today);
bln monday = time.isMonday(today);
bln tuesday = time.isTuesday(today);
bln wednesday = time.isWednesday(today);
bln thursday = time.isThursday(today);
bln friday = time.isFriday(today);
bln saturday = time.isSaturday(today);
bln sunday = time.isSunday(today);

str day = time.toDay(today);
str shortDay = time.toDayShort(today);
str monthName = time.toMonth(today);
str shortMonth = time.toMonthShort(today);
```

- `isWeekday`: Monday through Friday.
- `isWeekend`: Saturday or Sunday.
- `toDay`: English full weekday name, title-cased, e.g. `"Monday"`.
- `toDayShort`: English abbreviated weekday name, e.g. `"Mon"`.
- `toMonth`: English full month name, e.g. `"October"`.
- `toMonthShort`: English abbreviated month name, e.g. `"Oct"`.
- Locale-aware names are out of scope for V1; English names are deterministic.

### 5.3 Calendar boundaries

```grf
str today = time.today();
str weekStart = time.startOfWeek(today);
str weekEnd = time.endOfWeek(today);
str monthStart = time.startOfMonth(today);
str monthEnd = time.endOfMonth(today);
str yearStart = time.startOfYear(today);
str yearEnd = time.endOfYear(today);
```

All functions return ISO date strings. A week runs Monday through Sunday in V1.

### 5.4 Relative dates

```grf
str today = time.today();
str tomorrow = time.tomorrow();
str yesterday = time.yesterday();
str nextDay = time.addDays(today, 1);
str previousDay = time.addDays(today, -1);
str nextMonth = time.addMonths(today, 1);
str nextYear = time.addYears(today, 1);
str nextMonday = time.nextMonday(today);
str previousMonday = time.previousMonday(today);
```

Contracts:

- `tomorrow()` and `yesterday()` use the system-local calendar date.
- `addDays(dateText, amount)` performs calendar-day arithmetic.
- `addMonths(dateText, amount)` and `addYears(dateText, amount)` clamp to the last valid day when the original day does not exist in the target month/year. Example: January 31 plus one month becomes February 28, or February 29 in a leap year.
- `nextMonday(dateText)` means the Monday **strictly after** the input date. If the input is Monday, the result is seven days later.
- `previousMonday(dateText)` means the Monday **strictly before** the input date. If the input is Monday, the result is seven days earlier.
- Equivalent `nextTuesday`/`previousTuesday` etc. helpers may be added, but are not required for V1; `dayOfWeek` plus `addDays` can cover other cases.

### 5.5 Date comparisons

```grf
bln sameDay = time.isSameDay(dateA, dateB);
bln sameMonth = time.isSameMonth(dateA, dateB);
bln sameYear = time.isSameYear(dateA, dateB);
int days = time.daysBetween("2026-10-01", "2026-10-26");
```

- `isSameDay`, `isSameMonth`, and `isSameYear` compare the corresponding calendar components.
- `daysBetween(startDate, endDate)` returns `endDate - startDate` in calendar days. The result is signed; reversed arguments produce a negative result.
- `isPast` and `isFuture` accept parseable absolute timestamps, not date-only strings, unless the API explicitly defines a date-only overload later.
- No overloaded subtraction operator is required for strings.

## 6. Clock-time strings

The public representation for a clock time is a string, normally `HH:mm:ss` (24-hour clock). Optional milliseconds use `HH:mm:ss.SSS`.

```grf
str meeting = "14:30:00";
int hour = time.hour(meeting);
int minute = time.minute(meeting);
int second = time.second(meeting);
int ms = time.millisecond(meeting);
```

Functions:

| Function | Return type | Contract |
|---|---|---|
| `time.hour(timeText)` | `int` | Hour component, `0` through `23`. |
| `time.minute(timeText)` | `int` | Minute component, `0` through `59`. |
| `time.second(timeText)` | `int` | Second component, `0` through `59`. |
| `time.millisecond(timeText)` | `int` | Millisecond component, `0` through `999`; zero when omitted. |

Clock-time values do not identify a date or timezone. Functions requiring an absolute instant must receive a timestamp with sufficient date/time information.

## 7. Parsing and formatting

### 7.1 Parsing

```grf
str dateText = "2005-08-19";
str clockText = "14:30:00";
str isoText = "2026-10-26T14:30:00Z";

// Validate/normalize and return canonical strings.
str parsedDate = time.parseDate(dateText);
str parsedClock = time.parseClockTime(clockText);
str parsedIso = time.parse(isoText);

int timestampMs = time.parseInt(isoText);
flt timestampFlt = time.parseFlt(isoText);
```

- `parseDate` accepts a valid supported date and returns canonical `yyyy-MM-dd`.
- `parseClockTime` accepts a valid supported clock time and returns canonical `HH:mm:ss` or `HH:mm:ss.SSS` when fractional seconds are present.
- `parse` accepts a supported ISO 8601 timestamp with an explicit timezone/offset and returns canonical ISO 8601. It must not silently interpret an offset-less timestamp as UTC.
- `parseInt` and `parseFlt` convert an absolute timestamp to Unix milliseconds, integer and fractional respectively.
- Invalid input must produce the standard Graphite runtime error; no silent normalization of invalid dates such as February 30.

### 7.2 Formatting

```grf
str today = time.today();
str iso = time.iso();

str a = time.format(today, "dd/MM/yyyy");
str b = time.format(today, "EEEE, dd MMMM yyyy");
str c = time.format(iso, "yyyy-MM-dd HH:mm:ss");
str readable = time.str();
```

`format(value, pattern)` accepts a valid date, clock-time, or ISO timestamp string and formats the components available in that value. The input type/shape determines which fields are available. Requesting unavailable fields must raise a clear runtime error rather than inventing missing components.

Supported V1 tokens:

| Token | Meaning | Example |
|---|---|---|
| `yyyy` | Four-digit year | `2026` |
| `MM` | Two-digit month | `10` |
| `MMM` | Short month name | `Oct` |
| `MMMM` | Full month name | `October` |
| `dd` | Two-digit day | `26` |
| `EEEE` | Full weekday | `Monday` |
| `EEE` | Short weekday | `Mon` |
| `HH` | 24-hour hour | `16` |
| `hh` | 12-hour hour | `04` |
| `mm` | Minute | `30` |
| `ss` | Second | `15` |
| `SSS` | Milliseconds | `000` |
| `a` | AM/PM marker | `PM` |

The formatter must define literal escaping (recommended: single quotes delimit literals, with doubled single quote representing one literal quote). Unsupported tokens or malformed patterns must be rejected clearly. Token parsing should use longest-match behavior so `MMMM` is not treated as four `M` tokens.

### 7.3 Timezone formatting

```grf
str currentIso = time.iso();
str zone = time.timezone();
str local = time.formatIn(currentIso, "Asia/Kolkata", "yyyy-MM-dd HH:mm:ss");
```

- `formatIn(isoTimestamp, zoneId, pattern)` converts the represented instant to the requested IANA timezone for display; it does not change the underlying instant.
- Invalid timezone identifiers produce a runtime error.
- `timezone()` returns the detected system timezone identifier when available.
- V1 does not expose a timezone-bearing temporal type or a `toZone()` operation.

## 8. Unix timestamp conversion

```grf
str iso = "2026-10-26T14:30:00Z";
int ms = time.parseInt(iso);
flt fractionalMs = time.parseFlt(iso);

str backToIso = time.isoFromMs(ms);
str backToIsoPrecise = time.isoFromFlt(fractionalMs);
```

Use distinct conversion names for clarity:

- `parseInt(iso)` / `parseFlt(iso)`: timestamp string to epoch milliseconds.
- `isoFromMs(ms)` / `isoFromFlt(ms)`: epoch milliseconds to ISO timestamp.
- `time.int()` / `time.flt()`: current epoch milliseconds.

This avoids an ambiguous `fromMs` overload and keeps the API readable. If function overloading is already fully supported by Graphite, overloads can be considered later, but the named functions above are the V1 recommendation.

## 9. Duration helpers and elapsed calculations

V1 does not expose a public `duration` type. Durations can be represented as numeric milliseconds, while convenience functions create or convert quantities.

```grf
int halfSecond = time.millis(500);
int seconds = time.secs(30);
int minutes = time.mins(5);
int hours = time.hrs(2);
int dayMs = time.days(1); // fixed 24-hour duration

flt secondsValue = time.toSecs(halfSecond);
flt minutesValue = time.toMins(halfSecond);
flt hoursValue = time.toHrs(halfSecond);

int elapsedMs = time.elapsedMs(startMs, endMs);
```

Locked contracts:

- `millis(n)`, `secs(n)`, `mins(n)`, `hrs(n)`, and `days(n)` return a duration quantity in integer milliseconds.
- Duration creators (`millis`/`secs`/`mins`/`hrs`/`days`) accept **`int` only**.
- `toSecs` / `toMins` / `toHrs` take **`auto flt`**: `int` converts automatically; `flt` millisecond values (for example from `time.flt()` deltas) are accepted without an `int(...)` cast.
- `days(1)` means exactly **24 hours** of elapsed duration, not the next local calendar date. Calendar-day shifts use `addDays`.
- `toSecs(ms)`, `toMins(ms)`, and `toHrs(ms)` return `flt` so fractional units are retained.
- `elapsedMs(startMs, endMs)` returns `endMs - startMs`; it is a wall-clock calculation if supplied with `time.int()` values.
- Negative durations may be represented for arithmetic, but `sleep` / `sleepMs` reject negative values.

Because the public API uses numeric milliseconds, naming must make units explicit where practical. Do not add operators to strings for date/time arithmetic.

## 10. Duration arithmetic

Without a public duration type, arithmetic uses integer milliseconds:

```grf
int twoHours = time.hrs(2);
int halfHour = time.mins(30);
int totalMs = twoHours + halfHour;
```

For calendar changes, use `addDays`, `addMonths`, and `addYears` on ISO date strings. Do not implement calendar changes by adding a fixed millisecond count to a date string.

## 11. Monotonic timing and benchmarking (deferred)

Reliable benchmarking must not depend on wall-clock time. A type-free `tick()` / `elapsed()` API that returns opaque `int` handles is **deferred** for V1: integer tokens are too easy to confuse with Unix timestamps and need registry machinery Graphite does not yet have.

Until a suitable public representation exists (or a dedicated timer type is approved), document that:

- `time.flt()` / `time.int()` / `elapsedMs` are **wall-clock** measurements only.
- They can jump under clock corrections and are unsuitable as the sole basis for precise benchmarks.

Do not ship `tick` / `elapsed` in the phased V1 implementation.

## 12. Asynchronous sleep (locked)

Sleep is built on Graphite's existing `async<T>` / `await` feature and Tokio runtime. **Both** helpers ship:

```grf
import grf.time;

async<void> main() {
    await time.sleep(time.secs(4));
    await time.sleepMs(500);
}
```

- `time.sleep(milliseconds)` — takes an `int` duration in milliseconds (typically from `secs` / `mins` / …); returns `async<void>`; does not block the async executor thread.
- `time.sleepMs(milliseconds)` — convenience for a literal millisecond count; same return type and semantics.
- Negative duration is a runtime error; zero duration completes promptly.
- Library returns of `async<void>` are **approved**: extend the manifest and semantic analysis so `grf.json` can declare async library returns (required before exposing sleep).
- Use the existing Tokio runtime; do not create a nested runtime.
- Synchronous I/O and CPU work inside async functions can still block; `grf.time` does not promise async I/O.
- Sleep only works when awaited/polled under the runtime, consistent with Graphite async semantics.

## 13. Errors and validation

All public helpers must validate their inputs. Examples of runtime errors include:

- Invalid date or clock-time string.
- Invalid ISO timestamp or missing timezone where required.
- Invalid format pattern or unavailable field.
- Invalid timezone identifier.
- Timestamp conversion overflow or out-of-range value.
- Negative sleep duration.
- Undeterminable system timezone (`timezone()`).

Use Graphite's existing `Result<T, RuntimeError>` / error-propagation model (same as `grf.io`). Avoid a separate time-only error type. Function signatures and generated Rust must match current standard-library runtime error propagation.

## 14. Rust implementation guidance (locked)

- Keep internal Rust date/time values private to the runtime/library implementation.
- **Use `chrono`** for calendar validation, ISO parsing, formatting, and timezone work (add timezone support such as `chrono-tz` when `formatIn` / `timezone` need IANA zones).
- Use `std::time::SystemTime` (via chrono) for Unix timestamp access for `int()` / `flt()`.
- **Tokio `time` feature** for asynchronous sleep. Prefer centralizing dependencies on **`graphite-core`**; ensure generated packages enable the Tokio features they need when a program uses async (including sleep).
- Convert Graphite `str` inputs into validated internal representations, perform operations, then return Graphite `str`/numeric values.
- Do not implement calendar arithmetic through manual string slicing or arithmetic.
- Truncation for `time.int()` and `parseInt()`: toward zero after converting the epoch-millisecond value, with documented range checks.
- `time.flt()` precision is bounded by the host clock; it is not a promise of microsecond/nanosecond precision.

## 15. API inventory

The full planned API remains in this LLD. Implementation ships in phases (§18); later phases do not change the public contracts below.

### Planned public surface

- Current values: `flt`, `int`, `str`, `iso`, `today`, `now`, `timezone`.
- Date components: `year`, `month`, `day`, `dayOfWeek`, `dayOfYear`, `daysInMonth`, `isLeapYear`.
- Date predicates: `isFirstDayOfMonth`, `isLastDayOfMonth`, `isToday`, `isWeekday`, `isWeekend`, `isMonday`, `isTuesday`, `isWednesday`, `isThursday`, `isFriday`, `isSaturday`, `isSunday`.
- Names: `toDay`, `toDayShort`, `toMonth`, `toMonthShort`.
- Boundaries: `startOfWeek`, `endOfWeek`, `startOfMonth`, `endOfMonth`, `startOfYear`, `endOfYear`.
- Relative dates: `tomorrow`, `yesterday`, `addDays`, `addMonths`, `addYears`, `nextMonday`, `previousMonday`.
- Comparisons: `isSameDay`, `isSameMonth`, `isSameYear`, `daysBetween`, `isPast`, `isFuture`.
- Clock time: `hour`, `minute`, `second`, `millisecond`.
- Parsing/formatting: `parseDate`, `parseClockTime`, `parse`, `parseInt`, `parseFlt`, `format`, `formatIn`.
- Timestamp conversion: `isoFromMs`, `isoFromFlt`.
- Duration helpers: `millis`, `secs`, `mins`, `hrs`, `days`, `toSecs`, `toMins`, `toHrs`, `elapsedMs`.
- Async: `sleep`, `sleepMs`.

### Explicitly out of scope for V1

- Public Graphite types `date`, `clockTime`, `instant`, `duration`, or `timer`.
- `tick` / `elapsed` monotonic helpers (deferred; see §11).
- Locale-aware month/day names.
- Timezone-bearing date-time objects and timezone mutation/conversion APIs.
- Date/time operator overloading on strings.
- Async I/O APIs beyond sleep.
- Calendar/holiday databases.
- Parsing arbitrary natural-language dates.
- Public generic temporal wrapper types.

## 16. Testing requirements

Tests must cover:

1. `time.int()` and `time.flt()` return epoch milliseconds and have sensible precision/order properties.
2. `time.iso()` is valid ISO 8601 with UTC `Z` suffix.
3. `today()` and `now()` obey system-local timezone policy and canonical formats.
4. Leap years, century exceptions, February lengths, month/year boundaries, and invalid dates.
5. ISO weekday numbering (Monday=1, Sunday=7), weekday names, weekend/weekday predicates.
6. Week/month/year boundary functions.
7. Month/year clamping for dates such as January 31 and leap-day dates.
8. Signed `daysBetween` results and correct calendar-day behavior across daylight-saving transitions.
9. Parsing of timezone offsets and conversion to Unix milliseconds.
10. Formatter token matching, literal escaping, unsupported tokens, and unavailable fields.
11. Timezone formatting for known instants and invalid zone IDs.
12. Duration conversion units, rounding rules, and overflow behavior.
13. Async sleep completion, zero duration, negative duration errors, and correct runtime reuse.
14. Runtime error messages are stable enough for useful diagnostics, without leaking Rust implementation details.
15. `timezone()` errors when the system zone cannot be determined.

Phase tests can land with the matching implementation phase (§18).

## 17. Example end-to-end program

```grf
import grf.io;
import grf.time;

async<void> main() {
    str today = time.today();
    str weekday = time.toDay(today);
    str readable = time.format(today, "EEEE, dd MMMM yyyy");

    io.println("Today: " + readable);
    io.println("Weekday: " + weekday);

    flt start = time.flt();
    await time.sleepMs(250);
    flt end = time.flt();
    io.println("Wall-clock elapsed (ms): " + (end - start));
}
```

The example assumes string concatenation and numeric-to-string conversion follow existing Graphite rules. If not, use the language's established formatting/conversion helpers instead of adding time-specific behavior.

## 18. Locked decisions and phased implementation

### Locked decisions

| # | Topic | Decision |
|---|---|---|
| 1 | Local vs UTC | `iso()` / `int()` / `flt()` use Unix/UTC semantics; `today()` / `now()` / `str()` use system-local time. |
| 2 | V1 scope | Keep the full API in this LLD; implement essential functions first (phases below). |
| 3 | `tick` / `elapsed` | **Deferred.** Do not expose opaque timer handles as `int`. Revisit when Graphite has a suitable public representation. |
| 4 | Async library returns | **Approved.** Extend manifest + semantic analysis for `async<void>` library returns before shipping sleep. |
| 5 | Rust dependencies | **`chrono`** for date/time; Tokio **`time`** for async sleep; prefer deps on `graphite-core`; generated packages enable required Tokio features. |
| 6 | `timezone()` fallback | **Runtime error** if the system timezone cannot be determined. |
| 7 | Duration helpers | Creators (`millis`/`secs`/…) take **`int`**. Converters `toSecs`/`toMins`/`toHrs` take **`auto flt`** so both `int` and `flt` millisecond values work (e.g. `time.flt()` deltas). |
| 8 | Sleep API | Keep **both** `sleep(durationMs)` and `sleepMs(milliseconds)`. |

Central design unchanged: Graphite-facing `grf.time` uses existing primitives and helpers, not new public date/time types.

### Implementation status

**Shipped (recommended V1 set):**

- Current: `flt`, `int`, `str`, `iso`, `today`, `now`
- Date: `year`, `month`, `day`, `dayOfWeek`, `dayOfYear`, `daysInMonth`, `isLeapYear`, `isWeekday`, `isWeekend`, `toDay`, `toDayShort`, `toMonth`, `toMonthShort`, `isToday`, first/last day, same-day/month/year
- Arithmetic / boundaries: `tomorrow`, `yesterday`, `addDays`, `addWeeks`, `addMonths`, `addYears`, `daysBetween`, `startOfWeek`/`endOfWeek`, `startOfMonth`/`endOfMonth`, `startOfYear`/`endOfYear`, `nextMonday`, `previousMonday`
- Parse/format: `parseDate`, `parseClockTime`, `parse`, `parseInt`, `parseFlt`, `toMs`, `format`, `formatDate`, `formatClockTime`, `fromMs`, `fromFlt` (plus `isoFromMs`/`isoFromFlt` aliases)
- Durations (`auto flt` → `flt`): `ms`/`millis`, `secs`, `mins`, `hrs`, `days`, `toSecs`, `toMins`, `toHrs`, `toDays`, `elapsedMs`/`elapsedSecs`/`elapsedMins`/`elapsedHrs`
- Async: `sleep`, `sleepMs` (accept `auto flt` milliseconds)
- Preserve: `days(1)` is a fixed 24-hour duration; calendar day shifts use `addDays`

**Deferred:**

- `timezone`, `formatIn`, full weekday predicate family (`isMonday`…), `isPast`/`isFuture`, `monthsBetween`/`yearsBetween`, `tick`/`elapsed`, locale-aware names, `create`/`toUTC`/`toLocal` and other §9 extras not listed above
