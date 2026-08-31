#!/usr/bin/env ruby
# Protocol-v3 adapter for the two Ruby engines measured in Phase II RC1.
require "json"

PROTOCOL = "3.0"
ROOT = File.expand_path(__dir__)
DEPS = File.join(ROOT, ".adapter-deps")
$LOAD_PATH.unshift(*%W[
  #{DEPS}/tzinfo/lib
  #{DEPS}/concurrent-ruby/lib/concurrent-ruby
  #{DEPS}/raabro/lib
  #{DEPS}/et-orbi/lib
  #{DEPS}/fugit/lib
  #{DEPS}/ice_cube/lib
])
require "tzinfo"
require "fugit"
require "ice_cube"

def option(name)
  index = ARGV.index("--#{name}")
  index ? ARGV[index + 1] : nil
end

def emit(message)
  $stdout.puts(JSON.generate(message))
  $stdout.flush
end

def diagnostic(code, message)
  { "code" => code, "message" => message.to_s[0, 500] }
end

def tzdb_provenance
  %w[/usr/share/zoneinfo/tzdata.zi /usr/lib/zoneinfo/tzdata.zi].each do |path|
    next unless File.readable?(path)
    release = File.open(path, &:readline).strip.sub("# version ", "")
    return { "source" => "system zoneinfo", "release_kind" => "exact", "release" => release }
  end
  { "source" => "system zoneinfo", "release_kind" => "unknown" }
end

def offset_string(value)
  offset = value.utc_offset
  sign = offset >= 0 ? "+" : "-"
  offset = offset.abs
  format("%s%02d:%02d", sign, offset / 3600, (offset % 3600) / 60)
end

def format_zoned(value)
  value.strftime("%Y-%m-%dT%H:%M:%S") + offset_string(value) + "|" +
    value.getutc.strftime("%Y-%m-%dT%H:%M:%SZ")
end

def run_fugit(vector)
  input = vector["input"]
  zone = input["zone"]
  expression = zone ? "#{input['expr']} #{zone}" : input["expr"]
  cron = Fugit.parse_cron(expression)
  raise ArgumentError, "fugit parse returned nil" if cron.nil?
  timezone = EtOrbi.get_tzone(zone || "UTC")
  raise ArgumentError, "unknown zone #{zone.inspect}" if timezone.nil?
  current = EtOrbi.parse(input["start"].sub("T", " "), zone: timezone)
  output = []
  input["count"].times do
    current = cron.next_time(current)
    break if current.nil?
    output << (zone ? format_zoned(current) : current.strftime("%Y-%m-%dT%H:%M:%S"))
  end
  output
end

def run_ice_cube(vector)
  input = vector["input"]
  zone = input["zone"]
  schedule = IceCube::Schedule.from_ical(input["ics"])
  schedule.first(input["count"]).map do |value|
    zone ? format_zoned(value) : value.strftime("%Y-%m-%dT%H:%M:%S")
  end
end

engines = {
  "fugit" => {
    version: "git@efda655", provenance: "git floraison/fugit@efda655251c2ae86780f7e472a61653b5b4b528b",
    operations: %w[cron.next cron.parse], run: method(:run_fugit)
  },
  "ice_cube" => {
    version: "git@32ff145", provenance: "git seejohnrun/ice_cube@32ff145baf152ae4aa130376d66041eba174b085",
    operations: %w[rrule.expand rrule.parse], run: method(:run_ice_cube)
  }
}
engine_name = option("engine")
engine = engines[engine_name]
abort("unknown or missing --engine") if engine.nil?

emit(
  "message" => "hello", "protocol_version" => PROTOCOL,
  "runner" => { "name" => "occurframe-ruby-runner", "version" => "3.0.0",
                "provenance" => "source:runners/ruby/runner.rb" },
  "engine" => { "name" => engine_name, "version" => engine[:version],
                "provenance" => engine[:provenance] },
  "runtime" => { "language" => "Ruby", "runtime" => "MRI", "version" => RUBY_VERSION },
  "capabilities" => engine[:operations], "dialect_ids" => [],
  "semantic_profile_claims" => {}, "tzdb_provenance" => tzdb_provenance
)

$stdin.each_line do |line|
  next if line.strip.empty?
  begin
    message = JSON.parse(line)
    raise "expected protocol-v3 case" unless message["message"] == "case" && message["protocol_version"] == PROTOCOL
    request_id = message["request_id"]
    vector = {
      "id" => message["vector_id"],
      "family" => message["family"],
      "operation" => message["operation"],
      "input" => message["input"],
      "context" => message["semantic_context"]
    }
    operation = vector["operation"]
    probe = case operation
            when "cron.parse" then "cron.next"
            when "rrule.parse" then "rrule.expand"
            else operation
            end
    emit("message" => "started", "protocol_version" => PROTOCOL, "request_id" => request_id)
    outcome =
      if !engine[:operations].include?(operation) && !engine[:operations].include?(probe)
        { "type" => "unsupported", "diagnostic" => diagnostic(
          "unsupported_operation", "#{engine_name} does not implement #{operation}") }
      else
        begin
          occurrences = engine[:run].call(vector)
          operation.end_with?(".parse") ? { "type" => "accepted" } :
            { "type" => "occurrences", "occurrences" => occurrences }
        rescue ArgumentError, RangeError => exception
          { "type" => "rejection", "diagnostic" => diagnostic(
            "native_rejection", "#{exception.class}: #{exception.message}") }
        rescue StandardError => exception
          { "type" => "engine_error", "diagnostic" => diagnostic(
            "native_exception", "#{exception.class}: #{exception.message}") }
        end
      end
    emit("message" => "result", "protocol_version" => PROTOCOL,
         "request_id" => request_id, "outcome" => outcome, "warnings" => [])
  rescue StandardError => exception
    warn "protocol adapter failure: #{exception.class}: #{exception.message}"
    exit 1
  end
end
