"""
Benchmark: fastdbf vs dbf
=========================================
Compares read and write speeds of both libraries.

Prerequisites:
    uv add dbf fastdbf matplotlib seaborn --dev

Usage:
    uv run python benchmark.py
    uv run python benchmark.py --rows 50000
"""

import argparse
import os
import random
import string
import tempfile
import time
from dataclasses import dataclass

import matplotlib.pyplot as plt
import seaborn as sns

GLOBAL_SPECS = ""

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_rows(n: int, num_cols: int = 50) -> list[dict]:
    """Generates a specification string and n rows with num_cols fields."""
    global GLOBAL_SPECS
    specs_list = []
    fields_info = []
    
    for i in range(1, num_cols + 1):
        col_type = i % 4
        if col_type == 0:
            specs_list.append(f"COL{i} C(10)")
            fields_info.append((f"COL{i}", "C"))
        elif col_type == 1:
            specs_list.append(f"COL{i} N(10,2)")
            fields_info.append((f"COL{i}", "N"))
        elif col_type == 2:
            specs_list.append(f"COL{i} L")
            fields_info.append((f"COL{i}", "L"))
        else:
            specs_list.append(f"COL{i} D")
            fields_info.append((f"COL{i}", "D"))
            
    GLOBAL_SPECS = "; ".join(specs_list)
    
    rows = []
    for _ in range(n):
        row = {}
        for name, t in fields_info:
            if t == "C":
                row[name] = "".join(random.choices(string.ascii_letters, k=10))
            elif t == "N":
                row[name] = round(random.uniform(0.0, 1000.0), 2)
            elif t == "L":
                row[name] = random.choice([True, False])
            else:
                year = random.randint(1950, 2024)
                month = random.randint(1, 12)
                day = random.randint(1, 28)
                row[name] = f"{year:04d}-{month:02d}-{day:02d}"
        rows.append(row)
        
    return rows


@dataclass
class Result:
    label: str
    write_s: float
    read_s: float
    row_count: int
    file_size_bytes: int = 0

    @property
    def write_krows_s(self) -> float:
        return (self.row_count / 1000) / self.write_s

    @property
    def read_krows_s(self) -> float:
        return (self.row_count / 1000) / self.read_s


def fmt_size(n: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024:
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"


def estimate_csv_size(rows: list[dict]) -> int:
    """Estimates the CSV file size based on actual data."""
    import csv
    import io

    buf = io.StringIO()
    writer = csv.DictWriter(buf, fieldnames=rows[0].keys())
    writer.writeheader()
    writer.writerows(rows)
    return len(buf.getvalue().encode("utf-8"))


# ---------------------------------------------------------------------------
# fastdbf (Row-by-Row)
# ---------------------------------------------------------------------------

def bench_fastdbf_row_write(rows: list[dict], path: str) -> float:
    import fastdbf

    start = time.perf_counter()
    with fastdbf.Table(path, GLOBAL_SPECS, dbf_type="db3") as t:
        for row in rows:
            t.append(row)
    return time.perf_counter() - start


def bench_fastdbf_row_read(path: str) -> tuple[float, int]:
    import fastdbf

    start = time.perf_counter()
    with fastdbf.Table(path).open("r") as t:
        records = list(t)
    elapsed = time.perf_counter() - start
    return elapsed, len(records)


# ---------------------------------------------------------------------------
# fastdbf (Columnar Interface)
# ---------------------------------------------------------------------------

def bench_fastdbf_columnar_write(rows: list[dict], path: str) -> float:
    import fastdbf

    cols = {col: [r[col] for r in rows] for col in rows[0].keys()}
    start = time.perf_counter()
    with fastdbf.Table(path, GLOBAL_SPECS, dbf_type="db3") as t:
        t.extend_columns(cols)
    return time.perf_counter() - start


def bench_fastdbf_columnar_read(path: str) -> tuple[float, int]:
    import fastdbf
    import pandas as pd

    start = time.perf_counter()
    with fastdbf.Table(path).open("r") as t:
        cols = t.to_columns()
        df = pd.DataFrame(cols)
    elapsed = time.perf_counter() - start
    return elapsed, len(df)


# ---------------------------------------------------------------------------
# fastdbf (Arrow Interface)
# ---------------------------------------------------------------------------

def bench_fastdbf_arrow_write(rows: list[dict], path: str) -> float:
    import fastdbf
    import pandas as pd
    import pyarrow as pa

    df = pd.DataFrame(rows)
    batch = pa.RecordBatch.from_pandas(df)

    start = time.perf_counter()
    with fastdbf.Table(path, GLOBAL_SPECS, dbf_type="db3") as t:
        t.extend_arrow(batch)
    return time.perf_counter() - start


def bench_fastdbf_arrow_read(path: str) -> tuple[float, int]:
    import fastdbf
    import pandas as pd
    import pyarrow as pa

    start = time.perf_counter()
    with fastdbf.Table(path).open("r") as t:
        df = pa.Table.from_batches([pa.record_batch(t.to_arrow())]).to_pandas()
    elapsed = time.perf_counter() - start
    return elapsed, len(df)


# ---------------------------------------------------------------------------
# dbf (pure-Python reference implementation)
# ---------------------------------------------------------------------------

def bench_dbf_write(rows: list[dict], path: str) -> float:
    import dbf

    dbf_specs = GLOBAL_SPECS.lower()
    table = dbf.Table(path, dbf_specs, dbf_type="db3")
    table.open(dbf.READ_WRITE)
    
    date_fields = [
        part.split()[0].lower()
        for part in GLOBAL_SPECS.split("; ")
        if part.split()[1] == "D"
    ]
    
    start = time.perf_counter()
    for row in rows:
        cleaned = {}
        for k, v in row.items():
            k_lower = k.lower()
            if k_lower in date_fields:
                cleaned[k_lower] = dbf.Date.fromymd(v.replace("-", ""))
            else:
                cleaned[k_lower] = v
        table.append(cleaned)
    table.close()
    return time.perf_counter() - start


def bench_dbf_read(path: str) -> tuple[float, int]:
    import dbf

    start = time.perf_counter()
    table = dbf.Table(path)
    table.open(dbf.READ_ONLY)
    fields = table.field_names
    records = [{f: rec[f] for f in fields} for rec in table]
    table.close()
    elapsed = time.perf_counter() - start
    return elapsed, len(records)


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def run_benchmark(rows: list[dict], warmup: bool = True) -> list[Result]:
    results = []

    benches = [
        ("dbf", bench_dbf_write, bench_dbf_read),
        ("fastdbf (row)", bench_fastdbf_row_write, bench_fastdbf_row_read),
        ("fastdbf (columnar)", bench_fastdbf_columnar_write, bench_fastdbf_columnar_read),
        ("fastdbf (arrow)", bench_fastdbf_arrow_write, bench_fastdbf_arrow_read),
    ]

    for label, write_fn, read_fn in benches:
        with tempfile.NamedTemporaryFile(suffix=".dbf", delete=False) as f:
            path = f.name

        try:
            if warmup:
                write_fn(rows[:100], path)
                read_fn(path)

            write_s = write_fn(rows, path)
            file_size = os.path.getsize(path)
            read_s, count = read_fn(path)
            results.append(Result(label, write_s, read_s, count, file_size))
        except ImportError as exc:
            print(f"  [{label}] not installed - skipping ({exc})")
        except Exception as exc:
            print(f"  [{label}] error: {exc}")
        finally:
            try:
                os.unlink(path)
            except OSError:
                pass

    return results


# ---------------------------------------------------------------------------
# Output & Plotting
# ---------------------------------------------------------------------------


def plot_results(results: list[Result]):
    labels = [r.label for r in results]
    
    # 1. Speedup Plot
    dbf_write_time = next(r.write_s for r in results if r.label == "dbf")
    dbf_read_time = next(r.read_s for r in results if r.label == "dbf")
    
    write_speedups = [dbf_write_time / r.write_s for r in results]
    read_speedups = [dbf_read_time / r.read_s for r in results]

    sns.set_theme(style="whitegrid")

    fig, ax = plt.subplots(figsize=(10, 6))
    x = range(len(labels))
    width = 0.35

    ax.bar([i - width / 2 for i in x], write_speedups, width, label="Write Speedup (x)", color="salmon")
    ax.bar([i + width / 2 for i in x], read_speedups, width, label="Read Speedup (x)", color="skyblue")

    ax.set_ylabel("Speedup Factor (vs pure-Python dbf)")
    ax.set_title(f"Performance Speedup ({results[0].row_count:,} rows)")
    ax.set_xticks(x)
    ax.set_xticklabels(labels)
    ax.legend()

    plt.tight_layout()
    script_dir = os.path.dirname(os.path.abspath(__file__))
    plt.savefig(os.path.join(script_dir, "benchmark_speedup.png"), dpi=300)
    plt.close()
    print(f"Speedup plot saved to {os.path.join(script_dir, 'benchmark_speedup.png')}")

    # 2. Time Plot
    fig, ax = plt.subplots(figsize=(10, 6))
    
    write_times = [r.write_s for r in results]
    read_times = [r.read_s for r in results]

    ax.bar([i - width / 2 for i in x], write_times, width, label="Write Time (s)", color="salmon")
    ax.bar([i + width / 2 for i in x], read_times, width, label="Read Time (s)", color="skyblue")

    ax.set_ylabel("Time in Seconds (lower is better)")
    ax.set_title(f"Execution Time ({results[0].row_count:,} rows)")
    ax.set_xticks(x)
    ax.set_xticklabels(labels)
    ax.legend()

    plt.tight_layout()
    plt.savefig(os.path.join(script_dir, "benchmark_time.png"), dpi=300)
    plt.close()
    print(f"Time plot saved to {os.path.join(script_dir, 'benchmark_time.png')}")




def print_results(results: list[Result], rows: list[dict]) -> None:
    csv_size = estimate_csv_size(rows)

    col = 14
    header = (
        f"{'Package':<{col}}  {'Write':>12}  {'Read':>12}"
        f"  {'W kRows/s':>10}  {'R kRows/s':>10}  {'File Size':>12}  {'vs CSV':>8}"
    )
    print()
    print(header)
    print("-" * len(header))
    for r in results:
        ratio = r.file_size_bytes / csv_size if csv_size else 0
        print(
            f"{r.label:<{col}}"
            f"  {r.write_s:>11.3f}s"
            f"  {r.read_s:>11.3f}s"
            f"  {r.write_krows_s:>9.1f}k"
            f"  {r.read_krows_s:>9.1f}k"
            f"  {fmt_size(r.file_size_bytes):>12}"
            f"  {ratio:>7.1%}"
        )

    print(
        f"{'CSV (est.)':<{col}}  {'':>12}  {'':>12}"
        f"  {'':>10}  {'':>10}  {fmt_size(csv_size):>12}  {'100.0%':>8}"
    )
    print()

    # Comparison: all others vs fastdbf (first entry)
    baseline = results[0]
    for other in results[1:]:
        w_factor = other.write_s / baseline.write_s
        r_factor = other.read_s / baseline.read_s
        faster_w = "faster" if w_factor > 1 else "slower"
        faster_r = "faster" if r_factor > 1 else "slower"
        print(
            f"{baseline.label} is "
            f"{max(w_factor, 1 / w_factor):.1f}x {faster_w} than {other.label} at writing."
        )
        print(
            f"{baseline.label} is "
            f"{max(r_factor, 1 / r_factor):.1f}x {faster_r} than {other.label} at reading."
        )
        if baseline.file_size_bytes and other.file_size_bytes:
            s_factor = other.file_size_bytes / baseline.file_size_bytes
            print(
                f"{baseline.label} file is {max(s_factor, 1 / s_factor):.1f}x "
                f"{'larger' if s_factor > 1 else 'smaller'} than {other.label} file."
            )
        print()


# ---------------------------------------------------------------------------
# Entry Point
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="fastdbf vs dbf Benchmark")
    parser.add_argument(
        "--rows",
        type=int,
        default=100_000,
        help="Number of rows (default: 100000)",
    )
    parser.add_argument(
        "--no-warmup",
        action="store_true",
        help="Skip warmup run",
    )
    args = parser.parse_args()

    print(f"Generating {args.rows:,} rows...")
    rows = make_rows(args.rows)

    print(f"Starting benchmark (warmup={'no' if args.no_warmup else 'yes'})...")
    results = run_benchmark(rows, warmup=not args.no_warmup)
    print_results(results, rows)
    plot_results(results)


if __name__ == "__main__":
    main()
