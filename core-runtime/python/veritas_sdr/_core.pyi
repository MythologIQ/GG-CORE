"""Type stubs for veritas_sdr._core native module."""

from typing import Iterator, List, Optional

__version__: str

# Exceptions
class CoreError(Exception):
    """Base exception for all Veritas SDR errors."""
    pass

class AuthenticationError(CoreError):
    """Authentication or session validation failed."""
    pass

class InferenceError(CoreError):
    """Error during inference execution."""
    pass

class ModelError(CoreError):
    """Error loading or managing models."""
    pass

class TimeoutError(CoreError):
    """Operation timed out."""
    pass

class CancellationError(CoreError):
    """Operation was cancelled."""
    pass

# Classes
class Runtime:
    """Main runtime entry point for Veritas SDR."""

    def __init__(
        self,
        auth_token: str,
        base_path: Optional[str] = None,
        max_context_length: int = 4096,
        max_queue_depth: int = 1000,
    ) -> None:
        """Create a new runtime instance.

        Args:
            auth_token: Authentication token for session validation
            base_path: Optional path to models directory
            max_context_length: Maximum context window (default: 4096)
            max_queue_depth: Maximum request queue depth (default: 1000)
        """
        ...

    def session(self) -> Session:
        """Create a synchronous session."""
        ...

    def session_async(self) -> AsyncSession:
        """Create an async session."""
        ...

    def model_count(self) -> int:
        """Get number of loaded models."""
        ...

    def is_healthy(self) -> bool:
        """Check if runtime is healthy."""
        ...

    @staticmethod
    def version() -> str:
        """Get runtime version."""
        ...

class Session:
    """Synchronous session for inference operations."""

    def infer(
        self,
        model_id: int,
        tokens: List[int],
        params: Optional[InferenceParams] = None,
    ) -> InferenceResult:
        """Run inference on a model.

        Args:
            model_id: Model handle ID
            tokens: Input token IDs
            params: Optional inference parameters

        Returns:
            InferenceResult with output tokens
        """
        ...

    def infer_streaming(
        self,
        model_id: int,
        tokens: List[int],
        params: Optional[InferenceParams] = None,
    ) -> Iterator[StreamingResult]:
        """Run streaming inference.

        Yields tokens as they are generated.
        """
        ...

    def __enter__(self) -> Session:
        ...

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> bool:
        ...

class AsyncSession:
    """Async session for asyncio-based inference."""

    async def infer(
        self,
        model_id: int,
        tokens: List[int],
        params: Optional[InferenceParams] = None,
    ) -> InferenceResult:
        """Async inference."""
        ...

    def __aenter__(self) -> AsyncSession:
        ...

    def __aexit__(self, exc_type: object, exc_val: object, exc_tb: object) -> bool:
        ...

class InferenceParams:
    """Inference parameters for controlling generation."""

    max_tokens: int
    temperature: float
    top_p: float
    top_k: int
    stream: bool
    timeout_ms: Optional[int]

    def __init__(
        self,
        max_tokens: int = 256,
        temperature: float = 0.7,
        top_p: float = 0.9,
        top_k: int = 40,
        stream: bool = False,
        timeout_ms: Optional[int] = None,
    ) -> None:
        ...

class InferenceResult:
    """Result from inference operation."""

    tokens: List[int]
    finished: bool

    def __len__(self) -> int:
        ...

    def __getitem__(self, idx: int) -> int:
        ...

class StreamingResult:
    """A single streaming result chunk."""

    token: int
    index: int
    is_final: bool
    error: Optional[str]

    @property
    def is_error(self) -> bool:
        """Check if this result indicates an error."""
        ...

class ModelInfo:
    """Information about a loaded model."""

    name: str
    size_bytes: int

    def __init__(self, name: str, size_bytes: int) -> None:
        ...

    @property
    def size_human(self) -> str:
        """Get human-readable size string."""
        ...
