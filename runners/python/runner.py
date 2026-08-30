#!/usr/bin/env python3
"""Protocol-v2 adapter for the six Python engines measured in Phase II RC1."""

from __future__ import annotations

import argparse
import datetime as dt
import importlib.metadata as md
import json
import os
import sys

UTC = dt.timezone.utc
PROTOCOL = "2.0"
RUNNER = {"name": "occurframe-python-runner", "version": "2.0.0",
          "provenance": "source:runners/python/runner.py"}


def diagnostic(code, message):
    return {"code": code, "message": str(message)[:500]}


def emit(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":"), sort_keys=True) + "\n")
    sys.stdout.flush()


def off_str(value):
    seconds = int(value.utcoffset().total_seconds())
    sign = "+" if seconds >= 0 else "-"
    seconds = abs(seconds)
    return f"{sign}{seconds // 3600:02d}:{(seconds % 3600) // 60:02d}"


def fmt(value):
    if getattr(value, "tzinfo", None) is None:
        return value.strftime("%Y-%m-%dT%H:%M:%S")
    return (value.strftime("%Y-%m-%dT%H:%M:%S") + off_str(value) + "|" +
            value.astimezone(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"))


def setup_tz(mode):
    import zoneinfo
    if mode == "vendored":
        import tzdata
        zoneinfo.reset_tzpath([])
        return {
            "source": "python tzdata package",
            "release_kind": "exact",
            "release": tzdata.IANA_VERSION,
            "fingerprint": f"tzdata-{tzdata.__version__}",
        }
    for directory in zoneinfo.TZPATH:
        candidate = os.path.join(directory, "tzdata.zi")
        if os.path.exists(candidate):
            with open(candidate, encoding="utf-8") as handle:
                release = handle.readline().strip().replace("# version ", "")
            return {"source": "system zoneinfo", "release_kind": "exact", "release": release}
    return {"source": "system zoneinfo", "release_kind": "unknown"}


def parse_ics(ics):
    parsed = {"dtstart": None, "tzid": None, "rrule": [], "exrule": [],
              "rdate": [], "exdate": []}
    for line in ics.split("\n"):
        name, _, value = line.partition(":")
        params = name.split(";")
        key = params[0].upper()
        parameter_map = {}
        for parameter in params[1:]:
            pkey, _, pvalue = parameter.partition("=")
            parameter_map[pkey.upper()] = pvalue
        if key == "DTSTART":
            parsed["dtstart"] = value
            parsed["tzid"] = parameter_map.get("TZID")
        elif key == "RRULE":
            parsed["rrule"].append(value)
        elif key == "EXRULE":
            parsed["exrule"].append(value)
        elif key == "RDATE":
            parsed["rdate"].append((parameter_map, value))
        elif key == "EXDATE":
            parsed["exdate"].append((parameter_map, value))
    return parsed


def start_dt(vector):
    value = dt.datetime.fromisoformat(vector["input"]["start"])
    zone = vector["input"].get("zone")
    if zone:
        from zoneinfo import ZoneInfo
        return value.replace(tzinfo=ZoneInfo(zone))
    return value


class Croniter:
    ops = ("cron.next", "cron.parse")

    def __init__(self, day_or=True):
        from croniter import croniter as _croniter  # noqa: F401
        self.day_or = day_or
        self.name = "croniter" if day_or else "croniter[day_or=False]"
        self.version = "6.3.0.dev0"
        self.provenance = "git kiorky/croniter@3dd4d14e971294c03d3fb9be3f5ca03ae1c25310"

    def run(self, vector):
        from croniter import croniter
        value = vector["input"]
        iterator = croniter(value["expr"], start_dt(vector), day_or=self.day_or)
        return [fmt(iterator.get_next(dt.datetime)) for _ in range(value["count"])]


class Cronsim:
    name = "cronsim"
    version = "2.7"
    provenance = "git cuu508/cronsim@fd2e617787e94b15beee27fee6ebe6cbe79a72a2"
    ops = ("cron.next", "cron.parse")

    def run(self, vector):
        from cronsim import CronSim
        value = vector["input"]
        iterator = CronSim(value["expr"], start_dt(vector))
        output = []
        for _ in range(value["count"]):
            try:
                output.append(fmt(next(iterator)))
            except StopIteration:
                break
        return output


class APScheduler:
    name = "apscheduler3"
    version = "3.11.3"
    provenance = "git agronholm/apscheduler@4308ec95b94069f5dbdddb6c60fb792dfc8c40a4"
    ops = ("cron.next", "cron.parse")

    def run(self, vector):
        from apscheduler.triggers.cron import CronTrigger
        from zoneinfo import ZoneInfo
        value = vector["input"]
        zone = value.get("zone")
        timezone = ZoneInfo(zone) if zone else UTC
        trigger = CronTrigger.from_crontab(value["expr"], timezone=timezone)
        start = start_dt(vector)
        if start.tzinfo is None:
            start = start.replace(tzinfo=timezone)
        output, previous, current = [], None, start
        for _ in range(value["count"]):
            following = trigger.get_next_fire_time(previous, current)
            if following is None:
                break
            output.append(fmt(following if zone else following.replace(tzinfo=None)))
            previous, current = following, following + dt.timedelta(seconds=1)
        return output


class DateutilRRule:
    name = "python-dateutil"
    version = "2.9.0.post0"
    provenance = "PyPI python-dateutil==2.9.0.post0"
    ops = ("rrule.expand", "rrule.parse", "rrule.between")

    def __init__(self):
        actual = md.version("python-dateutil")
        if actual != self.version:
            raise RuntimeError(f"python-dateutil version mismatch: {actual}")

    def run(self, vector):
        from dateutil.rrule import rrulestr
        from dateutil.tz import gettz
        value = vector["input"]
        recurrence_set = rrulestr(value["ics"], forceset=True, unfold=True,
                                 tzids=lambda name: gettz(name))
        if vector["operation"] == "rrule.between":
            start, end = value["between"]
            zone = value.get("zone")
            first = dt.datetime.fromisoformat(start)
            last = dt.datetime.fromisoformat(end)
            if zone:
                first = first.replace(tzinfo=gettz(zone))
                last = last.replace(tzinfo=gettz(zone))
            return [fmt(item) for item in recurrence_set.between(first, last, inc=False)]
        output = []
        for item in recurrence_set:
            output.append(fmt(item))
            if len(output) >= value["count"]:
                break
        return output


class Pandas:
    name = "pandas"
    version = "3.0.2"
    provenance = "PyPI pandas==3.0.2"
    ops = ("rrule.expand",)

    def __init__(self):
        import pandas as pd
        if pd.__version__ != self.version:
            raise RuntimeError(f"pandas version mismatch: {pd.__version__}")

    def run(self, vector):
        import pandas as pd
        value = vector["input"]
        parsed = parse_ics(value["ics"])
        if len(parsed["rrule"]) != 1 or parsed["rdate"] or parsed["exdate"] or parsed["exrule"]:
            raise NotImplementedError("pandas recurrence sets unsupported")
        parts = dict(item.split("=", 1) for item in parsed["rrule"][0].split(";"))
        unsupported = set(parts) - {"FREQ", "COUNT", "INTERVAL"}
        if unsupported:
            raise NotImplementedError("pandas unsupported parts: " + ",".join(sorted(unsupported)))
        if "COUNT" not in parts:
            raise NotImplementedError("pandas unbounded rules unsupported")
        frequency = {"DAILY": "D", "WEEKLY": "W", "MONTHLY": "MS", "YEARLY": "YS",
                     "HOURLY": "h", "MINUTELY": "min", "SECONDLY": "s"}.get(parts["FREQ"])
        if frequency is None:
            raise NotImplementedError("pandas FREQ=" + parts["FREQ"])
        source = parsed["dtstart"]
        start = dt.datetime(int(source[0:4]), int(source[4:6]), int(source[6:8]),
                            int(source[9:11]), int(source[11:13]), int(source[13:15]))
        index = pd.date_range(start=start, periods=int(parts["COUNT"]),
                              freq=f"{int(parts.get('INTERVAL', 1))}{frequency}",
                              tz=parsed["tzid"])
        return [fmt(item.to_pydatetime()) for item in index][:value["count"]]


def create_engine(name):
    engines = {
        "croniter": lambda: Croniter(True),
        "croniter[day_or=False]": lambda: Croniter(False),
        "cronsim": Cronsim,
        "apscheduler3": APScheduler,
        "python-dateutil": DateutilRRule,
        "pandas": Pandas,
    }
    if name not in engines:
        raise ValueError(f"unknown engine {name}")
    return engines[name]()


def declarations(name):
    if name == "croniter":
        return ["cron.croniter-6-or@1"], {"cron.start_inclusivity": "exclusive"}
    if name == "croniter[day_or=False]":
        return ["cron.croniter-6-and@1"], {"cron.start_inclusivity": "exclusive"}
    if name == "apscheduler3":
        return ["cron.apscheduler-3@1"], {"cron.start_inclusivity": "inclusive"}
    return [], {}


def is_rejection(exception):
    return isinstance(exception, (ValueError, TypeError, KeyError, OverflowError)) or any(
        marker in type(exception).__name__.lower()
        for marker in ("croniterbadcron", "cronsimerror", "parseerror", "parsererror")
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", required=True)
    parser.add_argument("--tzdata", choices=("system", "vendored"), required=True)
    arguments = parser.parse_args()
    tzdb = setup_tz(arguments.tzdata)
    engine = create_engine(arguments.engine)
    dialects, profile = declarations(engine.name)
    emit({
        "message": "hello", "protocol_version": PROTOCOL,
        "runner": RUNNER,
        "engine": {"name": engine.name, "version": engine.version,
                   "provenance": engine.provenance},
        "runtime": {"language": "Python", "runtime": "CPython",
                    "version": sys.version.split()[0]},
        "capabilities": list(engine.ops), "dialect_ids": dialects,
        "semantic_profile_claims": profile, "tzdb_provenance": tzdb,
    })
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            case = json.loads(line)
            if case.get("message") != "case" or case.get("protocol_version") != PROTOCOL:
                raise ValueError("expected protocol-v2 case")
            request_id = case["request_id"]
            vector = case["vector"]
            operation = vector["operation"]
            probe = {"cron.parse": "cron.next", "rrule.parse": "rrule.expand"}.get(operation, operation)
            if operation not in engine.ops and probe not in engine.ops:
                emit({"message": "started", "protocol_version": PROTOCOL,
                      "request_id": request_id})
                outcome = {"type": "unsupported", "diagnostic": diagnostic(
                    "unsupported_operation", f"{engine.name} does not implement {operation}")}
            else:
                emit({"message": "started", "protocol_version": PROTOCOL,
                      "request_id": request_id})
                try:
                    occurrences = engine.run(vector)
                    outcome = ({"type": "accepted"} if operation.endswith(".parse") else
                               {"type": "occurrences", "occurrences": occurrences})
                except NotImplementedError as exception:
                    outcome = {"type": "unsupported", "diagnostic": diagnostic(
                        "unsupported_capability", exception)}
                except BaseException as exception:  # native engine boundary
                    kind = "rejection" if is_rejection(exception) else "engine_error"
                    code = "native_rejection" if kind == "rejection" else "native_exception"
                    outcome = {"type": kind, "diagnostic": diagnostic(code,
                               f"{type(exception).__name__}: {exception}")}
            emit({"message": "result", "protocol_version": PROTOCOL,
                  "request_id": request_id, "outcome": outcome, "warnings": []})
        except BaseException as exception:
            print(f"protocol adapter failure: {type(exception).__name__}: {exception}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
