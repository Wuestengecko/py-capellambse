# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0
"""An Eclipse-like XML serializer.

The libxml2 XML serializer produces very different output from the one
used by Capella. This causes a file saved by libxml2 to look vastly
different, even though semantically nothing might have changed at all.
This module implements a serializer which produces output like Capella
does.
"""

from __future__ import annotations

import contextlib
import os
import re
import sys
import typing as t
import warnings

import lxml.etree

from capellambse._compiled import serialize as _native_serialize

if sys.version_info >= (3, 13):
    from warnings import deprecated
else:
    from typing_extensions import deprecated


_UnspecifiedType = t.NewType("_UnspecifiedType", object)
_NOT_SPECIFIED = _UnspecifiedType(object())


@t.runtime_checkable
class HasWrite(t.Protocol):
    """A simple protocol to check for a writable file-like object."""

    def write(self, chunk: bytes) -> t.Any: ...


def to_string(tree: lxml.etree._Element, /) -> str:
    """Serialize an XML tree as a ``str``.

    No XML processing instruction will be inserted at the start of the
    document.

    Arguments
    ---------
    tree
        The XML tree to serialize.

    Returns
    -------
    str
        The serialized XML.
    """
    payload = serialize(tree, declare_encoding=False)
    return payload.decode("utf-8")


@deprecated("use serialize() with `declare_encoding=True` instead")
def to_bytes(
    tree: lxml.etree._Element,
    /,
    *,
    declare_encoding: bool = True,
) -> bytes:
    """Serialize an XML tree as a ``str``.

    At the start of the document, an XML processing instruction will be
    inserted declaring the used encoding. Pass
    ``declare_encoding=False`` to inhibit this behavior.

    Arguments
    ---------
    tree
        The XML tree to serialize.
    declare_encoding
        Whether to include an XML processing instruction declaring the
        encoding at the start of the document.

    Returns
    -------
    bytes
        The serialized XML, encoded using ``encoding``.
    """
    return serialize(tree, declare_encoding=declare_encoding)


def write(
    tree: lxml.etree._Element | lxml.etree._ElementTree,
    /,
    file: HasWrite | os.PathLike | str | bytes,
    *,
    line_length: float = LINE_LENGTH,
    siblings: bool = False,
    declare_encoding: bool = True,
) -> None:
    """Write the XML tree to ``file``.

    Parameters
    ----------
    tree
        The XML tree to serialize.
    file
        An open file or a PathLike to write the XML into.
    encoding
        The file encoding to use when opening a file.
    errors
        Set the encoding error handling behavior of newly opened files.
    line_length
        The number of characters after which to force a line break.
    siblings
        Also include siblings of the given subtree.
    declare_encoding
        Whether to include an XML processing instruction declaring the
        encoding at the start of the document.
    """
    ctx: t.ContextManager[HasWrite]
    if isinstance(file, HasWrite):
        ctx = contextlib.nullcontext(file)
    else:
        ctx = open(file, "wb")  # noqa: SIM115

    with ctx as f:
        serialize(
            tree,
            line_length=line_length,
            siblings=siblings,
            declare_encoding=declare_encoding,
            file=f,
        )


@t.overload
def serialize(
    tree: lxml.etree._Element | lxml.etree._ElementTree,
    /,
    *,
    line_length: float = ...,
    siblings: bool | None = ...,
    declare_encoding: bool = ...,
    file: None = ...,
) -> bytes: ...
@t.overload
def serialize(
    tree: lxml.etree._Element | lxml.etree._ElementTree,
    /,
    *,
    line_length: float = ...,
    siblings: bool | None = ...,
    declare_encoding: bool = ...,
    file: HasWrite,
) -> None: ...
def serialize(
    tree: lxml.etree._Element | lxml.etree._ElementTree,
    /,
    *,
    line_length: float = LINE_LENGTH,
    siblings: bool | None = None,
    declare_encoding: bool = False,
    file: HasWrite | None = None,
) -> bytes | None:
    """Serialize an XML tree.

    The iterator returned by this function yields the serialized XML
    piece by piece.

    Parameters
    ----------
    tree
        The XML tree to serialize.
    line_length
        The number of characters after which to force a line break.
    siblings
        Also include siblings of the given subtree. Defaults to yes if
        'tree' is an element tree, no if it's a single element.
    declare_encoding
        Whether to include an XML processing instruction declaring the
        encoding at the start of the document.
    file
        A file-like object to write the serialized tree to. If None, the
        serialized tree will be returned as bytes instead.

    Returns
    -------
    bytes | None
        The serialized tree (if no *file* was given), or None.
    """
    root: lxml.etree._Element
    if isinstance(tree, lxml.etree._ElementTree):
        if siblings is None:
            siblings = True
        root = tree.getroot()
    else:
        if siblings is None:
            siblings = False
        root = tree

    line_length = min(line_length, sys.maxsize)
    return _native_serialize(
        root,
        line_length=int(line_length),
        siblings=siblings,
        declare_encoding=declare_encoding,
        file=file,
    )


if not t.TYPE_CHECKING:

    def __getattr__(name):
        if name == "HAS_NATIVE":
            warnings.warn(
                "The native module is required and always available",
                DeprecationWarning,
                stacklevel=2,
            )
            return True

        escape_chars = r"[\x00-\x08\x0A-\x1F\x7F{}]"
        constants = {
            "INDENT": b"  ",
            "LINESEP": os.linesep.encode("ascii"),
            "LINE_LENGTH": 80,
            "ESCAPE_CHARS": escape_chars,
            "P_ESCAPE_TEXT": re.compile(escape_chars.format('"&<')),
            "P_ESCAPE_COMMENTS": re.compile(escape_chars.format(">")),
            "P_ESCAPE_ATTR": re.compile(escape_chars.format('"&<\x09')),
            "P_NAME": re.compile(r"^(?:\{([^}]*)\})?(.+)$"),
            "ALWAYS_EXPANDED_TAGS": frozenset({"bodies", "semanticResources"}),
        }
        if name in constants:
            warnings.warn(
                "Serialization-related constants in exs are deprecated",
                DeprecationWarning,
                stacklevel=2,
            )
            return constants[name]

        raise AttributeError(name)
