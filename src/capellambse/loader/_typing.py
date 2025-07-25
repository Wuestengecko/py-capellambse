# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0

__all__ = [
    "Element",
    "FragmentType",
    "Loader",
    "ModelInfo",
]

import collections.abc as cabc
import contextlib
import dataclasses
import enum
import pathlib
import typing as t

import typing_extensions as te
from lxml import etree

from capellambse import filehandler

_E = t.TypeVar("_E", bound="Element")
_Q = t.TypeVar("_Q")
Loader: t.TypeAlias = (
    "_Loader[Element, etree.QName] | _Loader[etree._Element, etree.QName]"
)


class FragmentType(enum.Enum):
    """The type of an XML fragment."""

    SEMANTIC = enum.auto()
    VISUAL = enum.auto()
    OTHER = enum.auto()


@dataclasses.dataclass
class ModelInfo:
    url: str | None
    title: str | None
    entrypoint: pathlib.PurePosixPath
    resources: dict[str, filehandler.abc.HandlerInfo]
    capella_version: str
    viewpoints: dict[str, str]


class _Tree(t.Protocol[_E, _Q]):
    @property
    def root(self) -> _E: ...
    @property
    def fragment_type(self) -> FragmentType: ...

    def iterall(self, /) -> cabc.Iterator[_E]: ...
    def iter_qtypes(self, /) -> cabc.Iterator[_Q]: ...
    def iter_qtype(self, qtype: _Q, /) -> cabc.Iterator[_E]: ...

    def add_namespace(self, uri: str, alias: str, /) -> str: ...


class _Loader(t.Protocol, t.Generic[_E, _Q]):
    @property
    def trees(
        self,
    ) -> cabc.MutableMapping[pathlib.PurePosixPath, _Tree[_E, _Q]]: ...
    @property
    def resources(self) -> dict[str, filehandler.FileHandler]: ...

    def get_model_info(self, /) -> ModelInfo: ...

    def find_fragment(self, elem: _E, /) -> pathlib.PurePosixPath: ...
    def iterancestors(self, elem: _E, /) -> cabc.Iterator[_E]: ...
    def iterdescendants(self, elem: _E, /) -> cabc.Iterator[_E]: ...
    def iterchildren(self, elem: _E, tag: str, /) -> cabc.Iterator[_E]: ...
    def find_references(self, target_id: str, /) -> cabc.Iterator[_E]: ...

    def create_link(
        self,
        source: _E,
        target: _E,
        *,
        include_target_type: bool | None = None,
    ) -> str: ...
    def follow_link(self, source: _E | None, id: str, /) -> _E: ...
    def follow_links(
        self,
        source: _E,
        id_list: str,
        /,
        *,
        ignore_broken: bool = ...,
    ) -> list[_E]: ...

    def new_uuid(
        self,
        parent: _E,
        /,
        *,
        want: str | None = ...,
    ) -> contextlib.AbstractContextManager[str]: ...
    def idcache_index(self, subtree: _E, /) -> None: ...
    def idcache_remove(self, subtree: _E, /) -> None: ...
    def idcache_rebuild(self, /) -> None: ...

    def activate_viewpoint(self, name: str, version: str, /) -> None: ...
    def update_namespaces(self, /) -> None: ...
    def save(self, /, **kw: t.Any) -> None: ...

    def write_tmp_project_dir(
        self, /
    ) -> contextlib.AbstractContextManager[pathlib.Path]: ...


class Element(t.Protocol):
    @property
    def tag(self) -> str: ...

    def iterchildren(self, tag: str = ..., /) -> cabc.Iterator[te.Self]: ...


if t.TYPE_CHECKING:

    def __protocol_compliance_check() -> None:
        from capellambse import loader  # noqa: PLC0415

        tree: _Tree
        tree = loader.ModelFile()  # type: ignore[call-arg]
        del tree

        elm: Element
        elm = etree._Element()
        del elm

        ldr: Loader
        ldr = loader.MelodyLoader()  # type: ignore[call-arg]
        del ldr
