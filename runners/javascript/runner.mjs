#!/usr/bin/env bun
// Protocol-v2 adapter for the six JavaScript configurations measured in Phase II RC1.
import readline from 'node:readline'
import { CronExpressionParser } from 'cron-parser'
import { Cron } from 'croner'
import cronstrue from 'cronstrue'
import { rrulestr } from 'rrule'
import { DateTime } from 'luxon'

const PROTOCOL = '2.0'
const ENGINE_NAME = option('engine')
if (!ENGINE_NAME) throw new Error('--engine is required')

function option(name) {
  const index = process.argv.indexOf('--' + name)
  return index < 0 ? undefined : process.argv[index + 1]
}

function emit(message) {
  process.stdout.write(JSON.stringify(message) + '\n')
}

function diagnostic(code, message) {
  return { code, message: String(message).slice(0, 500) }
}

function offsetString(minutes) {
  const sign = minutes >= 0 ? '+' : '-'
  const absolute = Math.abs(minutes)
  return `${sign}${String(Math.floor(absolute / 60)).padStart(2, '0')}:${String(absolute % 60).padStart(2, '0')}`
}

function formatZoned(date, zone) {
  const value = DateTime.fromJSDate(date).setZone(zone)
  const local = value.toFormat("yyyy-LL-dd'T'HH:mm:ss") + offsetString(value.offset)
  return local + '|' + date.toISOString().replace(/\.\d{3}Z$/, 'Z')
}

function formatNaive(date) {
  return date.toISOString().slice(0, 19)
}

function fingerprintTzdb() {
  const offset = (zone, iso) => DateTime.fromISO(iso, { zone }).offset
  const vancouver = offset('America/Vancouver', '2026-11-02T12:00')
  const edmonton = offset('America/Edmonton', '2026-11-02T12:00')
  const casablanca = offset('Africa/Casablanca', '2026-09-21T12:00')
  if (vancouver === -480) return { label: 'le2026a', min: null, max: '2026a' }
  if (edmonton === -420 || casablanca === 60) return { label: '2026b', min: '2026b', max: '2026b' }
  return { label: 'ge2026c', min: '2026c', max: null }
}

function tzdbProvenance() {
  const exposed = process.versions?.tz
  if (exposed) {
    return { source: 'runtime ICU', release_kind: 'exact', release: exposed }
  }
  const fingerprint = fingerprintTzdb()
  const value = {
    source: 'runtime ICU', release_kind: 'bounded',
    fingerprint: `TZDB-001/002/003:${fingerprint.label}`,
  }
  if (fingerprint.min) value.min_inclusive = fingerprint.min
  if (fingerprint.max) value.max_inclusive = fingerprint.max
  return value
}

function cronParserRun(vector, strict) {
  const input = vector.input
  const options = { currentDate: input.start, strict }
  if (input.zone) options.tz = input.zone
  const iterator = CronExpressionParser.parse(input.expr, options)
  const output = []
  for (let index = 0; index < input.count; index += 1) {
    const date = iterator.next().toDate()
    output.push(input.zone ? formatZoned(date, input.zone) : formatNaive(date))
  }
  return output
}

function cronerRun(vector, legacyMode) {
  const input = vector.input
  const options = {}
  if (legacyMode !== undefined) options.legacyMode = legacyMode
  if (input.zone) options.timezone = input.zone
  const cron = new Cron(input.expr, options)
  let previous = input.zone
    ? DateTime.fromISO(input.start, { zone: input.zone }).toJSDate()
    : new Date(input.start + 'Z')
  const output = []
  for (let index = 0; index < input.count; index += 1) {
    previous = cron.nextRun(previous)
    if (!previous) break
    output.push(input.zone ? formatZoned(previous, input.zone) : formatNaive(previous))
  }
  return output
}

function rruleRun(vector) {
  const input = vector.input
  const set = rrulestr(input.ics, { forceset: true })
  if (vector.operation === 'rrule.between') {
    const toDate = value => input.zone
      ? DateTime.fromISO(value, { zone: input.zone }).toJSDate()
      : new Date(value + 'Z')
    return set.between(toDate(input.between[0]), toDate(input.between[1]), false)
      .map(date => input.zone ? formatZoned(date, input.zone) : formatNaive(date))
  }
  const output = []
  set.all((date, length) => {
    output.push(date)
    return length < input.count
  })
  return output.slice(0, input.count)
    .map(date => input.zone ? formatZoned(date, input.zone) : formatNaive(date))
}

const ENGINES = {
  'cron-parser': {
    version: '5.10.0', provenance: 'git harrisiirak/cron-parser@7b3a0ad748bffd6eaf6af4caac4d83b1fc392378',
    operations: ['cron.next', 'cron.parse'], dialects: [], profile: {},
    run: vector => cronParserRun(vector, false),
  },
  'cron-parser[strict]': {
    version: '5.10.0', provenance: 'git harrisiirak/cron-parser@7b3a0ad748bffd6eaf6af4caac4d83b1fc392378',
    operations: ['cron.next', 'cron.parse'], dialects: [], profile: {},
    run: vector => cronParserRun(vector, true),
  },
  croner: {
    version: '10.0.1', provenance: 'git Hexagon/croner@713ee7217e3bbb01857559199e312149d2695edb',
    operations: ['cron.next', 'cron.parse'], dialects: ['cron.croner-10@1'], profile: {},
    run: vector => cronerRun(vector, undefined),
  },
  'croner[legacyMode=false]': {
    version: '10.0.1', provenance: 'git Hexagon/croner@713ee7217e3bbb01857559199e312149d2695edb',
    operations: ['cron.next', 'cron.parse'], dialects: ['cron.croner-10@1'], profile: {},
    run: vector => cronerRun(vector, false),
  },
  cronstrue: {
    version: '3.24.0', provenance: 'git bradymholt/cRonstrue@b62884a10cc76705c53be65210784108a6d337dd',
    operations: ['cron.next', 'cron.parse'], dialects: [], profile: {},
    run: vector => ['DESCRIPTION:' + cronstrue.toString(vector.input.expr, { throwExceptionOnParseError: true })],
  },
  'rrule.js': {
    version: '2.8.0', provenance: 'git jkbrzt/rrule@9f2061febeeb363d03352efe33d30c33073a0242',
    operations: ['rrule.expand', 'rrule.parse', 'rrule.between'], dialects: [], profile: {},
    run: rruleRun,
  },
}

const engine = ENGINES[ENGINE_NAME]
if (!engine) throw new Error(`unknown engine ${ENGINE_NAME}`)
const runtime = globalThis.Bun
  ? { language: 'JavaScript', runtime: 'Bun', version: globalThis.Bun.version }
  : { language: 'JavaScript', runtime: 'Node.js', version: process.versions.node }

emit({
  message: 'hello', protocol_version: PROTOCOL,
  runner: { name: 'occurframe-javascript-runner', version: '2.0.0', provenance: 'source:runners/javascript/runner.mjs' },
  engine: { name: ENGINE_NAME, version: engine.version, provenance: engine.provenance },
  runtime, capabilities: engine.operations, dialect_ids: engine.dialects,
  semantic_profile_claims: engine.profile, tzdb_provenance: tzdbProvenance(),
})

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity })
for await (const line of input) {
  if (!line.trim()) continue
  try {
    const message = JSON.parse(line)
    if (message.message !== 'case' || message.protocol_version !== PROTOCOL) {
      throw new Error('expected protocol-v2 case')
    }
    const vector = message.vector
    const operation = vector.operation
    const probe = operation === 'cron.parse' ? 'cron.next'
      : operation === 'rrule.parse' ? 'rrule.expand' : operation
    emit({ message: 'started', protocol_version: PROTOCOL, request_id: message.request_id })
    let outcome
    if (!engine.operations.includes(operation) && !engine.operations.includes(probe)) {
      outcome = { type: 'unsupported', diagnostic: diagnostic('unsupported_operation', `${ENGINE_NAME} does not implement ${operation}`) }
    } else {
      try {
        outcome = { type: 'occurrences', occurrences: engine.run(vector) }
      } catch (error) {
        const name = error?.constructor?.name ?? 'Error'
        const deliberate = ['Error', 'RangeError', 'TypeError', 'String'].includes(name)
        outcome = {
          type: deliberate ? 'rejection' : 'engine_error',
          diagnostic: diagnostic(deliberate ? 'native_rejection' : 'native_exception', `${name}: ${error?.message ?? error}`),
        }
      }
    }
    emit({ message: 'result', protocol_version: PROTOCOL, request_id: message.request_id, outcome, warnings: [] })
  } catch (error) {
    console.error(`protocol adapter failure: ${error?.stack ?? error}`)
    process.exit(1)
  }
}
