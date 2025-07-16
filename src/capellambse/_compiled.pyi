# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0

from typing import Protocol

import awesomeversion as av
from lxml import etree

import capellambse.model as m

class _HasWrite(Protocol):
    def write(self, _: bytes, /) -> None: ...

def serialize(
    tree: etree._Element,
    /,
    *,
    line_length: int,
    siblings: bool,
    declare_encoding: bool,
    file: _HasWrite | None,
) -> bytes: ...

class Namespace:
    @property
    def uri(self) -> str: ...
    @property
    def alias(self) -> str: ...
    @property
    def viewpoint(self) -> str | None: ...
    @property
    def maxver(self) -> av.AwesomeVersion | None: ...
    @property
    def version_precision(self) -> int: ...
    def __init__(
        self,
        uri: str,
        alias: str,
        viewpoint: str | None = ...,
        maxver: str | None = ...,
        *,
        version_precision: int = ...,
    ) -> None: ...
    def match_uri(self, uri: str, /) -> bool | av.AwesomeVersion | None: ...
    def get_class(
        self,
        clsname: str,
        /,
        version: str | None = ...,
    ) -> type[m.ModelElement]: ...
    def register(
        self,
        cls: type[m.ModelElement],
        /,
        minver: str | None,
        maxver: str | None,
    ) -> None: ...
    def trim_version(self, version: str, /) -> str: ...
    def __contains__(self, clsname: str, /) -> bool: ...
