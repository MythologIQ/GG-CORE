"""Veritas SDR - Secure Deterministic Runtime for Python

A sandboxed, offline inference engine for LLM execution.

Example usage:

    import veritas_sdr

    # Create runtime with authentication
    runtime = veritas_sdr.Runtime(auth_token="your-secret-token")

    # Sync session
    with runtime.session() as session:
        result = session.infer(model_id=1, tokens=[1, 2, 3])
        print(result.tokens)

    # Streaming
    with runtime.session() as session:
        for chunk in session.infer_streaming(model_id=1, tokens=[1, 2, 3]):
            print(chunk.token)
"""

from ._core import (
    # Main classes
    Runtime,
    Session,
    AsyncSession,
    InferenceParams,
    InferenceResult,
    StreamingResult,
    ModelInfo,
    # Exceptions
    CoreError,
    AuthenticationError,
    InferenceError,
    ModelError,
    TimeoutError,
    CancellationError,
    # Metadata
    __version__,
)

__all__ = [
    # Main classes
    "Runtime",
    "Session",
    "AsyncSession",
    "InferenceParams",
    "InferenceResult",
    "StreamingResult",
    "ModelInfo",
    # Exceptions
    "CoreError",
    "AuthenticationError",
    "InferenceError",
    "ModelError",
    "TimeoutError",
    "CancellationError",
    # Metadata
    "__version__",
]
