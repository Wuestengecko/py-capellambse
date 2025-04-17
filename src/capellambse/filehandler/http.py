# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import collections.abc as cabc
import errno
import itertools
import logging
import os
import pathlib
import re
import typing as t
import urllib.parse

import httpx
import typing_extensions as te

from capellambse import helpers

from . import abc

LOGGER = logging.getLogger(__name__)

NOT_FOUND_CODES = (
    400,  # Bad Request
    404,  # Not Found
    410,  # Gone
    418,  # I'm a teapot
)


class DownloadStream(t.BinaryIO):
    def __init__(
        self, client: httpx.Client, url: str, chunk_size: int = 1024**2
    ) -> None:
        LOGGER.debug("Opening HTTP download stream from %s", url)
        self.url = url
        self.chunk_size = chunk_size
        self.__client = client

        self.__stream = self.__client.stream("GET", self.url)
        self.__response = resp = self.__stream.__enter__()
        self.__buffer = memoryview(b"")

        phrase = httpx.codes.get_reason_phrase(resp.status_code)
        LOGGER.debug("Status: %d %s", resp.status_code, phrase)
        if resp.status_code in NOT_FOUND_CODES:
            raise FileNotFoundError(
                errno.ENOENT,
                f"Got {resp.status_code} {phrase} for URL {self.url}",
            )
        resp.raise_for_status()

        self.__iterator = resp.iter_bytes(self.chunk_size)

    def __enter__(self) -> DownloadStream:
        return self

    def __exit__(self, *args: t.Any) -> None:
        self.close()

    def read(self, n: int = -1) -> bytes:
        if n == -1:
            return b"".join(itertools.chain((self.__buffer,), self.__iterator))

        if not self.__buffer:
            try:
                self.__buffer = memoryview(next(self.__iterator))
            except StopIteration:
                return b""

        chunk = bytes(self.__buffer[:n])
        self.__buffer = self.__buffer[n:]
        return chunk

    def readable(self) -> bool:
        return True

    def write(self, s: bytes | bytearray) -> int:  # type: ignore[override]
        del s
        raise TypeError("Cannot write to a read-only stream")

    def writable(self) -> bool:
        return False

    def close(self) -> None:
        del self.__stream
        del self.__buffer
        self.__stream.__exit__(None, None, None)


class HTTPFileHandler(abc.FileHandler):
    """A remote file handler that fetches files using HTTP GET."""

    def __init__(
        self,
        path: str | os.PathLike,
        username: str | None = None,
        password: str | None = None,
        *,
        headers: cabc.Mapping[str, str] | None = None,
        subdir: str | pathlib.PurePosixPath = "/",
    ) -> None:
        """Connect to a remote server through HTTP or HTTPS.

        This file handler supports two ways of specifying a URL:

        1. If a plain URL is passed, the requested file name is appended
           after a forward slash ``/``.
        2. The URL may contain one or more of the following escape
           sequences to provide more fine-grained control over how and
           where the file name is inserted into the URL:

           - ``%s``: The path to the file, with everything except
             forward slashes percent-escaped
           - ``%q``: The path to the file, with forward slashes percent
             escaped as well
           - ``%d``: The directory name, without trailing slash, like %s
           - ``%n``: The file name without extension
           - ``%e``: The file extension without leading dot
           - ``%%``: A literal percent sign

        Examples: When requesting the file name ``demo/my model.aird``,
        ...

        - ``https://example.com/~user`` as ``path`` results in the URL
          ``https://example.com/~user/demo/my%20model.aird``
        - ``https://example.com/~user/%s`` results in
          ``https://example.com/~user/demo/my%20model.aird``
        - ``https://example.com/?file=%q`` results in
          ``https://example.com/?file=demo%2Fmy%20model.aird``

        Note that the file name that is inserted into the URL will never
        start with a forward slash. This means that a URL like
        ``https://example.com%s`` will not work; you need to hard-code
        the slash at the appropriate place.

        This also applies to the ``%q`` escape. If the server expects
        the file name argument to start with a slash, hard-code a
        percent-escaped slash in the URL. For example, instead of
        ``...?file=%q`` use ``...?file=%2F%q``.

        Parameters
        ----------
        path
            The base URL to fetch files from. Must start with
            ``http://`` or ``https://``. See above for how to specify
            complex URLs.
        username
            The username for HTTP Basic Auth.
        password
            The password for HTTP Basic Auth.
        headers
            Additional HTTP headers to send to the server.
        subdir
            Prepend this path to all requested files. It is subject to
            the same file name escaping rules explained above.
        """
        if not isinstance(path, str):
            raise TypeError(
                "HTTPFileHandler requires a str path, not"
                f" {type(path).__name__}"
            )
        if bool(username) != bool(password):
            raise ValueError(
                "Either both username and password must be given, or neither"
            )

        if not re.search("%[%a-z]", path):
            path = path.rstrip("/") + "/%s"

        super().__init__(path, subdir=subdir)

        if username and password:
            auth = (username, password)
        else:
            auth = None
        self.client = httpx.Client(auth=auth, headers=headers)

    def open(
        self,
        filename: str | pathlib.PurePosixPath,
        mode: t.Literal["r", "rb", "w", "wb"] = "rb",
    ) -> t.BinaryIO:
        if "w" in mode:
            raise NotImplementedError("Cannot upload to HTTP(S) locations")
        assert isinstance(self.path, str)
        fname = self.subdir / helpers.normalize_pure_path(filename)
        fname_str = str(fname).lstrip("/")
        q = urllib.parse.quote
        replace = {
            "%s": q(fname_str, safe="/"),
            "%q": q(fname_str, safe=""),
            "%d": q(str(fname.parent).lstrip("/")),
            "%n": q(fname.with_suffix("").name),
            "%e": q(fname.suffix.lstrip(".")),
            "%%": "%",
        }
        url = re.sub("%[%a-z]", lambda m: replace[m.group(0)], self.path)
        assert url != self.path
        return DownloadStream(  # type: ignore[abstract] # false-positive
            self.client, url
        )

    def write_transaction(self, **kw: t.Any) -> t.NoReturn:
        raise NotImplementedError(
            "Write transactions for HTTP(S) are not implemented"
        )

    def iterdir(  # pragma: no cover
        self, path: str | pathlib.PurePosixPath = ".", /
    ) -> cabc.Iterator[abc.FilePath[te.Self]]:
        del path
        raise TypeError(
            "Cannot list files on raw HTTP sources."
            " Maybe you forgot a 'git+' prefix?"
        )
