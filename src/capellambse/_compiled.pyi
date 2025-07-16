# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0

import os
from collections.abc import Iterable, Mapping
from typing import Any, Generic, Protocol, overload

import awesomeversion as av
from lxml import etree
from typing_extensions import Never, Self

import capellambse.model as m
from capellambse import filehandler

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

class NativeLoader:
    def __init__(
        self,
        path: str | os.PathLike | filehandler.FileHandler,
        **kwargs: Any,
    ) -> None: ...

class ElementList:
    def __new__(cls, _: Never, /) -> Self: ...

class Containment(Generic[m.T_co]):
    def __init__(
        self,
        role: str | None,
        class_: m.UnresolvedClassName,
        /,
        *,
        mapkey: str | None = ...,
        mapvalue: str | None = ...,
        alternate: type[m.ModelElement] | None = ...,
        single_attr: str | None = ...,
        fixed_length: int = ...,
        type_hint_map: Mapping[str, m.UnresolvedClassName] | None = ...,
    ) -> None: ...
    @overload
    def __get__(self, obj: None, objtype: type[m.T_co]) -> Self: ...
    @overload
    def __get__(
        self, obj: m.ModelObject, objtype: type[m.T_co]
    ) -> m.ElementList[m.T_co]: ...
    def __set__(
        self,
        obj: m.ModelObject,
        value: Iterable[str | m.T_co | m.NewObject],
    ) -> None: ...
    def __delete__(self, obj: m.ModelObject) -> None: ...

class Association(Generic[m.T_co]):
    def __init__(
        self,
        class_: type[m.T_co] | None | m.UnresolvedClassName,
        attr: str | None,
        /,
        *,
        mapkey: str | None = ...,
        mapvalue: str | None = ...,
        fixed_length: int = ...,
    ) -> None: ...
    @overload
    def __get__(self, obj: None, objtype: type[m.ModelElement]) -> Self: ...
    @overload
    def __get__(
        self, obj: m.ModelElement, objtype: type[m.ModelElement]
    ) -> m.ElementList[m.T_co]: ...
    def __set__(
        self,
        obj: m.ModelObject,
        value: Iterable[str | m.T_co | m.NewObject],
    ) -> None: ...
    def __delete__(self, obj: m.ModelObject) -> None: ...

class Backref(Generic[m.T_co]):
    def __init__(
        self,
        class_: m.UnresolvedClassName,
        attr0: str,
        /,
        *attrs: str,
        mapkey: str | None = ...,
        mapvalue: str | None = ...,
    ) -> None: ...
    @overload
    def __get__(self, obj: None, objtype: type[m.ModelElement]) -> Self: ...
    @overload
    def __get__(
        self, obj: m.ModelObject, objtype: type[m.ModelElement]
    ) -> m.ElementList[m.T_co]: ...
    def __set__(
        self,
        obj: m.ModelObject,
        value: Iterable[str | m.T_co | m.NewObject],
    ) -> None: ...
    def __delete__(self, obj: m.ModelObject) -> None: ...

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
