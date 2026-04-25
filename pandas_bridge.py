"""
fastdbf.pandas_bridge
=====================
Pandas-Integration für fastdbf.

Verwendung:
    from fastdbf.pandas_bridge import DbfDataFrame

    # DBF lesen
    ddf = DbfDataFrame.from_dbf("meine.dbf")
    print(ddf.data)

    # DataFrame schreiben
    import pandas as pd
    df = pd.DataFrame({"NAME": ["Alice", "Bob"], "AGE": [30, 25]})
    ddf = DbfDataFrame.from_dataframe(df)
    ddf.to_dbf("ausgabe.dbf")

Voraussetzungen:
    pip install pandas fastdbf
"""

from __future__ import annotations

import logging
from collections import OrderedDict
from collections.abc import Iterable
from pathlib import Path
from typing import Any

import pandas as pd

logger = logging.getLogger(__name__)

# Maximale Länge für Character-Felder (DBF-Limit)
MAX_STRING_LENGTH = 254

# Standardwerte
DEFAULT_DBF_TYPE = "vfp"


# ---------------------------------------------------------------------------
# Typinferenz: pandas dtype → DBF field spec
# ---------------------------------------------------------------------------

def _infer_field_spec(series: pd.Series, name: str) -> str:
    """Leitet den DBF-Feldtyp aus einer pandas-Series ab."""
    dtype = series.convert_dtypes().dtype

    if isinstance(dtype, pd.BooleanDtype):
        return "L"
    if isinstance(dtype, pd.Int8Dtype | pd.Int16Dtype | pd.Int32Dtype | pd.Int64Dtype |
                        pd.UInt8Dtype | pd.UInt16Dtype | pd.UInt32Dtype | pd.UInt64Dtype):
        return "I"
    if isinstance(dtype, pd.Float32Dtype | pd.Float64Dtype):
        return "B"
    if isinstance(dtype, pd.StringDtype):
        max_len = int(series.dropna().str.len().max()) if not series.dropna().empty else 1
        max_len = min(max(max_len, 1), MAX_STRING_LENGTH)
        return f"C({max_len})"

    # numpy dtypes als Fallback
    kind = getattr(dtype, "kind", None)
    if kind == "b":
        return "L"
    if kind in ("i", "u"):
        return "I"
    if kind == "f":
        return "B"
    if kind in ("U", "O", "S"):
        try:
            max_len = int(series.dropna().astype(str).str.len().max())
        except Exception:
            max_len = MAX_STRING_LENGTH
        max_len = min(max(max_len, 1), MAX_STRING_LENGTH)
        return f"C({max_len})"

    # Alles-NULL-Spalte
    if series.isna().all():
        logger.warning(
            "Spalte '%s' enthält nur NULL-Werte. Verwende C(%d). "
            "Übergib einen expliziten field_spec um das zu überschreiben.",
            name, MAX_STRING_LENGTH,
        )
        return f"C({MAX_STRING_LENGTH})"

    raise NotImplementedError(
        f"Kein DBF-Feldtyp für Spalte '{name}' (dtype={dtype}) implementiert. "
        f"Bitte field_spec manuell angeben."
    )


def _nullable_suffix(series: pd.Series) -> str:
    return " null" if series.isna().any() else ""


# ---------------------------------------------------------------------------
# Wertkonvertierung: Python/pandas → fastdbf-kompatibler Wert
# ---------------------------------------------------------------------------

def _to_dbf_value(value: Any, spec: str) -> Any:
    """Konvertiert einen pandas-Wert in einen fastdbf-kompatiblen Wert."""
    if pd.isna(value):
        return None

    spec_upper = spec.strip().upper()

    if spec_upper.startswith("L"):
        return bool(value)
    if spec_upper.startswith("I"):
        return int(value)
    if spec_upper.startswith("B"):
        return float(value)
    if spec_upper.startswith("N") or spec_upper.startswith("F"):
        return float(value)
    if spec_upper.startswith("D"):
        # pandas Timestamp oder datetime.date → ISO-String YYYY-MM-DD
        if hasattr(value, "strftime"):
            return value.strftime("%Y-%m-%d")
        return str(value)[:10]
    if spec_upper.startswith("C"):
        return str(value)

    return value


# ---------------------------------------------------------------------------
# Wertkonvertierung: fastdbf-Wert → pandas-kompatibler Wert
# ---------------------------------------------------------------------------

def _from_dbf_value(value: Any) -> Any:
    """Konvertiert einen fastdbf-Wert in einen pandas-kompatiblen Wert."""
    if value is None:
        return pd.NA
    # fastdbf liefert Strings mit trailing whitespace für Character-Felder
    if isinstance(value, str):
        return value.rstrip()
    return value


# ---------------------------------------------------------------------------
# Hauptklasse
# ---------------------------------------------------------------------------

class DbfDataFrame:
    """Wrapper der fastdbf und pandas verbindet.

    Kapselt einen pandas.DataFrame zusammen mit den DBF-Feldspezifikationen
    und bietet Methoden zum Lesen und Schreiben von .dbf-Dateien.

    Attribute:
        data:        Der pandas.DataFrame mit den Tabellendaten.
        field_specs: OrderedDict {FELDNAME: "Typ-Spec"} z.B. {"NAME": "C(10)", "AGE": "I"}.
    """

    def __init__(
        self,
        *,
        data: pd.DataFrame,
        field_specs: OrderedDict[str, str],
        dbf_type: str = DEFAULT_DBF_TYPE,
    ):
        self._data = data
        self._field_specs = field_specs
        self._dbf_type = dbf_type

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def data(self) -> pd.DataFrame:
        return self._data

    @property
    def field_specs(self) -> OrderedDict[str, str]:
        return self._field_specs

    @property
    def dbf_type(self) -> str:
        return self._dbf_type

    # ------------------------------------------------------------------
    # Konstruktoren
    # ------------------------------------------------------------------

    @classmethod
    def from_dbf(
        cls,
        filename: str | Path,
        *,
        dbf_type: str = DEFAULT_DBF_TYPE,
        field_specs: dict[str, str] | None = None,
    ) -> DbfDataFrame:
        """Liest eine .dbf-Datei und gibt ein DbfDataFrame zurück.

        Args:
            filename:    Pfad zur .dbf-Datei.
            dbf_type:    DBF-Typ (z.B. "vfp", "db3"). Standard: "vfp".
            field_specs: Optionales Dict zum Überschreiben einzelner Feldspezifikationen.
                         Beispiel: {"ID": "C(36)"} setzt die Länge des ID-Feldes auf 36.

        Returns:
            DbfDataFrame mit den gelesenen Daten.

        Raises:
            ValueError: Wenn field_specs Felder enthält die nicht in der DBF existieren.
        """
        import fastdbf

        filename = Path(filename)
        table = fastdbf.open_table(str(filename))

        # Feldnamen und Feldinfo aus der Tabelle lesen
        raw_fields = table.fields()  # Liste von Dicts mit name, type_code, length, decimals, nullable
        col_names = [f["name"] for f in raw_fields]

        # Rohdaten lesen: Liste von Dicts
        raw_records = list(table)

        # DataFrame aufbauen
        data = pd.DataFrame(
            [{col: _from_dbf_value(rec[col]) for col in col_names} for rec in raw_records],
            columns=col_names,
        )

        # Typen optimieren
        data = data.convert_dtypes()

        # String-Spalten trimmen (DBF padded mit Leerzeichen)
        for col in data.columns:
            if isinstance(data[col].dtype, pd.StringDtype):
                data[col] = data[col].str.strip()

        # Feldspezifikationen aus der Tabellenstruktur ableiten
        inferred = cls._field_specs_from_table(raw_fields)

        # Optionale Overrides anwenden
        if field_specs is not None:
            unknown = set(field_specs.keys()) - set(inferred.keys())
            if unknown:
                raise ValueError(
                    f"field_specs enthält unbekannte Felder: {unknown}. "
                    f"Vorhandene Felder: {list(inferred.keys())}"
                )
            inferred.update(field_specs)

        return cls(data=data, field_specs=inferred, dbf_type=dbf_type)

    @classmethod
    def from_dataframe(
        cls,
        data: pd.DataFrame,
        *,
        dbf_type: str = DEFAULT_DBF_TYPE,
        field_specs: dict[str, str] | None = None,
    ) -> DbfDataFrame:
        """Erstellt ein DbfDataFrame aus einem pandas.DataFrame.

        Feldspezifikationen werden automatisch aus den dtypes abgeleitet.
        Mit field_specs können einzelne Felder manuell gesetzt werden.

        Args:
            data:        Der pandas.DataFrame.
            dbf_type:    DBF-Typ. Standard: "vfp".
            field_specs: Optionales Dict zum Überschreiben einzelner Feldspezifikationen.
                         Beispiel: {"ID": "C(36) null"} für ein nullbares 36-Zeichen-Feld.

        Returns:
            DbfDataFrame bereit zum Schreiben.

        Raises:
            ValueError: Wenn Spaltennamen nicht case-insensitiv eindeutig sind.
            ValueError: Wenn field_specs Spalten enthält die nicht im DataFrame sind.
            NotImplementedError: Wenn ein dtype nicht unterstützt wird.
        """
        if field_specs is None:
            field_specs = {}

        # Prüfe ob Spalten case-insensitiv eindeutig sind (DBF-Anforderung)
        upper_names = [col.upper() for col in data.columns]
        if len(set(upper_names)) != len(upper_names):
            raise ValueError(
                "Spaltennamen müssen case-insensitiv eindeutig sein (DBF-Anforderung)."
            )

        # Prüfe ob field_specs nur bekannte Spalten enthält
        unknown = set(field_specs.keys()) - set(data.columns)
        if unknown:
            raise ValueError(
                f"field_specs enthält unbekannte Spalten: {unknown}. "
                f"Vorhandene Spalten: {list(data.columns)}"
            )

        inferred: OrderedDict[str, str] = OrderedDict()
        for col in data.columns:
            if col in field_specs:
                inferred[col.upper()] = field_specs[col]
            else:
                spec = _infer_field_spec(data[col], col)
                spec += _nullable_suffix(data[col])
                inferred[col.upper()] = spec

        return cls(data=data, field_specs=inferred, dbf_type=dbf_type)

    # ------------------------------------------------------------------
    # Schreiben
    # ------------------------------------------------------------------

    def to_dbf(self, path: str | Path, *, exists_ok: bool = True) -> None:
        """Schreibt den DataFrame als .dbf-Datei.

        Args:
            path:      Ausgabepfad.
            exists_ok: Wenn False, wird ein Fehler geworfen falls die Datei schon existiert.

        Raises:
            FileExistsError: Wenn die Datei existiert und exists_ok=False.
        """
        import fastdbf

        path = Path(path)
        if not exists_ok and path.exists():
            raise FileExistsError(f"Datei '{path}' existiert bereits.")

        # field_specs als Semikolon-getrennte Zeichenkette für fastdbf
        spec_str = "; ".join(
            f"{name} {spec}" for name, spec in self._field_specs.items()
        )

        table = fastdbf.create_table(
            spec_str,
            filename=str(path),
            on_disk=True,
            dbf_type=self._dbf_type,
        )
        table.open()

        # Vorberechnete Listen für den heißen Pfad – vermeidet dict-Lookups pro Zeile
        field_names = list(self._field_specs.keys())
        field_specs = list(self._field_specs.values())
        col_indices = list(range(len(field_names)))

        for row in self._data.itertuples(index=False, name=None):
            record = {
                field_names[i]: _to_dbf_value(row[i], field_specs[i])
                for i in col_indices
            }
            table.append(record)

        table.close()

    # ------------------------------------------------------------------
    # DataFrame-Manipulation
    # ------------------------------------------------------------------

    def add_columns(
        self,
        data: pd.DataFrame | pd.Series,
        *,
        field_specs: dict[str, str],
    ) -> None:
        """Fügt neue Spalten zum DataFrame und den Feldspezifikationen hinzu.

        Args:
            data:        Die neuen Spalten als DataFrame oder Series.
            field_specs: Pflichtangabe: Feldspezifikationen für alle neuen Spalten.

        Raises:
            ValueError: Wenn die Länge nicht übereinstimmt.
            ValueError: Wenn field_specs unvollständig ist.
        """
        data = pd.DataFrame(data)

        if len(data) != len(self._data):
            raise ValueError(
                f"Datenlänge {len(data)} stimmt nicht mit Tabellenlänge {len(self._data)} überein."
            )

        missing = set(data.columns) - set(field_specs.keys())
        if missing:
            raise ValueError(f"Fehlende field_specs für Spalten: {missing}")

        for col in data.columns:
            self._data[col] = data[col]
            self._field_specs[col.upper()] = field_specs[col]

    def drop_columns(self, columns: Iterable[str]) -> None:
        """Entfernt Spalten aus dem DataFrame und den Feldspezifikationen.

        Args:
            columns: Spaltennamen die entfernt werden sollen.
        """
        for col in columns:
            self._data = self._data.drop(col, axis=1)
            self._field_specs.pop(col.upper(), None)

    def update_field_specs(self, field_specs: dict[str, str]) -> None:
        """Aktualisiert Feldspezifikationen für bestehende Spalten.

        Args:
            field_specs: Dict {SPALTENNAME: neue_spec}.

        Raises:
            ValueError: Wenn ein Spaltenname nicht existiert.
        """
        for name, spec in field_specs.items():
            key = name.upper()
            if key not in self._field_specs:
                raise ValueError(
                    f"Spalte '{name}' nicht gefunden. "
                    f"Vorhandene Spalten: {list(self._field_specs.keys())}"
                )
            self._field_specs[key] = spec

    # ------------------------------------------------------------------
    # Hilfsmethoden
    # ------------------------------------------------------------------

    @staticmethod
    def _field_specs_from_table(raw_fields: list[dict]) -> OrderedDict[str, str]:
        """Baut field_specs aus der fastdbf-Feldinformation auf."""
        specs: OrderedDict[str, str] = OrderedDict()
        type_map = {
            "C": lambda f: f"C({f['length']})",
            "N": lambda f: f"N({f['length']},{f['decimals']})",
            "F": lambda f: f"F({f['length']},{f['decimals']})",
            "D": lambda f: "D",
            "L": lambda f: "L",
            "M": lambda f: "M",
            "I": lambda f: "I",
            "B": lambda f: "B",
            "T": lambda f: "T",
            "Y": lambda f: "Y",
            "G": lambda f: "G",
            "P": lambda f: "P",
        }
        for field in raw_fields:
            name = field["name"]
            code = field["type_code"]
            builder = type_map.get(code)
            spec = builder(field) if builder else f"{code}({field['length']})"
            if field.get("nullable"):
                spec += " null"
            specs[name] = spec
        return specs

    def __repr__(self) -> str:
        rows, cols = self._data.shape
        return (
            f"DbfDataFrame("
            f"rows={rows}, cols={cols}, "
            f"dbf_type={self._dbf_type!r}, "
            f"fields={list(self._field_specs.keys())}"
            f")"
        )