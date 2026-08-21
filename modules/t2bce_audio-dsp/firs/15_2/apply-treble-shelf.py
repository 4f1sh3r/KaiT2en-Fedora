#!/usr/bin/env python3

from pathlib import Path

import numpy as np
from scipy.io import wavfile
from scipy.signal import lfilter


GAIN_DB = -3.0
FREQUENCY_HZ = 6000.0
SHELF_SLOPE = 1.0


def high_shelf(sample_rate: int) -> tuple[np.ndarray, np.ndarray]:
    amplitude = 10.0 ** (GAIN_DB / 40.0)
    omega = 2.0 * np.pi * FREQUENCY_HZ / sample_rate
    cosine = np.cos(omega)
    alpha = np.sin(omega) / 2.0 * np.sqrt(
        (amplitude + 1.0 / amplitude) * (1.0 / SHELF_SLOPE - 1.0) + 2.0
    )
    root_amplitude = np.sqrt(amplitude)

    b = np.array(
        [
            amplitude
            * ((amplitude + 1.0) + (amplitude - 1.0) * cosine + 2.0 * root_amplitude * alpha),
            -2.0 * amplitude * ((amplitude - 1.0) + (amplitude + 1.0) * cosine),
            amplitude
            * ((amplitude + 1.0) + (amplitude - 1.0) * cosine - 2.0 * root_amplitude * alpha),
        ]
    )
    a = np.array(
        [
            (amplitude + 1.0) - (amplitude - 1.0) * cosine + 2.0 * root_amplitude * alpha,
            2.0 * ((amplitude - 1.0) - (amplitude + 1.0) * cosine),
            (amplitude + 1.0) - (amplitude - 1.0) * cosine - 2.0 * root_amplitude * alpha,
        ]
    )
    return b / a[0], a / a[0]


def main() -> None:
    destination = Path(__file__).resolve().parent
    source = destination.parent / "16_2"

    for source_path in sorted(source.glob("*.wav")):
        sample_rate, impulse = wavfile.read(source_path)
        if impulse.dtype != np.float32 or impulse.ndim != 1:
            raise ValueError(f"unexpected WAV format: {source_path}")

        b, a = high_shelf(sample_rate)
        filtered = lfilter(b, a, impulse.astype(np.float64))
        wavfile.write(destination / source_path.name, sample_rate, filtered.astype(np.float32))


if __name__ == "__main__":
    main()
