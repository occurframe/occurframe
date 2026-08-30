<?php
/** Protocol-v2 adapter for the two PHP engines measured in Phase II RC1. */
declare(strict_types=1);

const PROTOCOL_VERSION = '2.0';
$engineName = option('engine');
if ($engineName === null) {
    fwrite(STDERR, "--engine is required\n");
    exit(2);
}

$sourceRoot = __DIR__ . '/.adapter-deps';
spl_autoload_register(function (string $class) use ($sourceRoot): void {
    $prefixes = [
        'Cron\\' => $sourceRoot . '/php-cron-expression/src/Cron/',
        'RRule\\' => $sourceRoot . '/php-rrule/src/',
    ];
    foreach ($prefixes as $prefix => $directory) {
        if (strncmp($class, $prefix, strlen($prefix)) === 0) {
            $relative = str_replace('\\', '/', substr($class, strlen($prefix)));
            $path = $directory . $relative . '.php';
            if (is_file($path)) {
                require $path;
            }
            return;
        }
    }
});

function option(string $name): ?string {
    global $argv;
    $index = array_search('--' . $name, $argv, true);
    return $index === false ? null : ($argv[$index + 1] ?? null);
}

function emit(array $message): void {
    $encoded = json_encode($message, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR);
    fwrite(STDOUT, $encoded . "\n");
    fflush(STDOUT);
}

function diagnostic(string $code, string $message): array {
    return ['code' => $code, 'message' => substr($message, 0, 500)];
}

function tzdbProvenance(): array {
    $version = timezone_version_get();
    if ($version !== '0.system') {
        return ['source' => 'PHP bundled tzdb', 'release_kind' => 'exact', 'release' => $version];
    }
    foreach (['/usr/share/zoneinfo/tzdata.zi', '/usr/lib/zoneinfo/tzdata.zi'] as $path) {
        if (is_readable($path)) {
            $handle = fopen($path, 'r');
            $line = trim((string)fgets($handle));
            fclose($handle);
            return ['source' => 'system zoneinfo', 'release_kind' => 'exact',
                    'release' => str_replace('# version ', '', $line)];
        }
    }
    return ['source' => 'system zoneinfo', 'release_kind' => 'unknown'];
}

function offsetString(DateTimeInterface $value): string {
    $offset = $value->getOffset();
    $sign = $offset >= 0 ? '+' : '-';
    $offset = abs($offset);
    return sprintf('%s%02d:%02d', $sign, intdiv($offset, 3600), intdiv($offset % 3600, 60));
}

function formatZoned(DateTimeInterface $value): string {
    $utc = (clone $value)->setTimezone(new DateTimeZone('UTC'));
    return $value->format('Y-m-d\TH:i:s') . offsetString($value) . '|' . $utc->format('Y-m-d\TH:i:s') . 'Z';
}

function formatNaive(DateTimeInterface $value): string {
    return $value->format('Y-m-d\TH:i:s');
}

function runCronExpression(array $vector): array {
    $input = $vector['input'];
    $zone = $input['zone'] ?? null;
    $timezone = new DateTimeZone($zone ?? 'UTC');
    $expression = new Cron\CronExpression($input['expr']);
    $current = new DateTime($input['start'], $timezone);
    $output = [];
    for ($index = 0; $index < $input['count']; $index++) {
        $following = $expression->getNextRunDate($current, 0, false, $zone ?? 'UTC');
        $output[] = $zone ? formatZoned($following) : formatNaive($following);
        $current = $following;
    }
    return $output;
}

function runPhpRRule(array $vector): array {
    $input = $vector['input'];
    $zone = $input['zone'] ?? null;
    $set = new RRule\RSet($input['ics']);
    $output = [];
    if ($vector['operation'] === 'rrule.between') {
        $timezone = new DateTimeZone($zone ?? 'UTC');
        $first = new DateTime($input['between'][0], $timezone);
        $last = new DateTime($input['between'][1], $timezone);
        foreach ($set as $value) {
            if ($value <= $first) continue;
            if ($value >= $last) break;
            $output[] = $zone ? formatZoned($value) : formatNaive($value);
        }
        return $output;
    }
    foreach ($set as $value) {
        $output[] = $zone ? formatZoned($value) : formatNaive($value);
        if (count($output) >= $input['count']) break;
    }
    return $output;
}

$engines = [
    'php-cron-expression' => [
        'version' => '3.x@d425a24',
        'provenance' => 'git dragonmantank/cron-expression@d425a2403c17d7cf911c55a7170f073979a9f382',
        'operations' => ['cron.next', 'cron.parse'],
        'run' => 'runCronExpression',
    ],
    'php-rrule' => [
        'version' => '2.x@93a083d',
        'provenance' => 'git rlanvin/php-rrule@93a083db12dcb6f58e4840392a22e158ce96f1ff',
        'operations' => ['rrule.expand', 'rrule.parse', 'rrule.between'],
        'run' => 'runPhpRRule',
    ],
];
if (!array_key_exists($engineName, $engines)) {
    fwrite(STDERR, "unknown engine $engineName\n");
    exit(2);
}
$engine = $engines[$engineName];
emit([
    'message' => 'hello', 'protocol_version' => PROTOCOL_VERSION,
    'runner' => ['name' => 'occurframe-php-runner', 'version' => '2.0.0',
                 'provenance' => 'source:runners/php/runner.php'],
    'engine' => ['name' => $engineName, 'version' => $engine['version'],
                 'provenance' => $engine['provenance']],
    'runtime' => ['language' => 'PHP', 'runtime' => 'PHP', 'version' => PHP_VERSION],
    'capabilities' => $engine['operations'], 'dialect_ids' => [],
    'semantic_profile_claims' => new stdClass(), 'tzdb_provenance' => tzdbProvenance(),
]);

while (($line = fgets(STDIN)) !== false) {
    if (trim($line) === '') continue;
    try {
        $case = json_decode($line, true, 512, JSON_THROW_ON_ERROR);
        if (($case['message'] ?? null) !== 'case' || ($case['protocol_version'] ?? null) !== PROTOCOL_VERSION) {
            throw new RuntimeException('expected protocol-v2 case');
        }
        $requestId = $case['request_id'];
        $vector = $case['vector'];
        $operation = $vector['operation'];
        $probe = $operation === 'cron.parse' ? 'cron.next'
               : ($operation === 'rrule.parse' ? 'rrule.expand' : $operation);
        emit(['message' => 'started', 'protocol_version' => PROTOCOL_VERSION,
              'request_id' => $requestId]);
        if (!in_array($operation, $engine['operations'], true) &&
            !in_array($probe, $engine['operations'], true)) {
            $outcome = ['type' => 'unsupported', 'diagnostic' => diagnostic(
                'unsupported_operation', "$engineName does not implement $operation")];
        } else {
            try {
                $occurrences = ($engine['run'])($vector);
                $outcome = ['type' => 'occurrences', 'occurrences' => $occurrences];
            } catch (InvalidArgumentException | OutOfRangeException | DomainException $exception) {
                $outcome = ['type' => 'rejection', 'diagnostic' => diagnostic(
                    'native_rejection', get_class($exception) . ': ' . $exception->getMessage())];
            } catch (Throwable $exception) {
                $outcome = ['type' => 'engine_error', 'diagnostic' => diagnostic(
                    'native_exception', get_class($exception) . ': ' . $exception->getMessage())];
            }
        }
        emit(['message' => 'result', 'protocol_version' => PROTOCOL_VERSION,
              'request_id' => $requestId, 'outcome' => $outcome, 'warnings' => []]);
    } catch (Throwable $exception) {
        fwrite(STDERR, 'protocol adapter failure: ' . get_class($exception) . ': ' . $exception->getMessage() . "\n");
        exit(1);
    }
}
