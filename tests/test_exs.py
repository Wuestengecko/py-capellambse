# SPDX-FileCopyrightText: Copyright DB InfraGO AG
# SPDX-License-Identifier: Apache-2.0
from __future__ import annotations

import os
import sys

import pytest
from lxml import etree

from capellambse.loader import exs

LF = os.linesep


@pytest.mark.parametrize(
    "string",
    [
        pytest.param(
            f"""<p title="&#x9;&amp;Hello, &lt;&quot;World&quot;>!"/>{LF}""",
            id="attribute",
        ),
        pytest.param(
            f"""<p>\t&amp;Hello, &lt;&quot;World&quot;>!</p>{LF}""",
            id="text",
        ),
        pytest.param(
            f"""{LF}<!--\t&Hello, <"World"&gt;!-->{LF}<p/>{LF}""",
            id="comment",
        ),
    ],
)
def test_escaping(string: str) -> None:
    tree = etree.fromstring(string)
    expected = string.encode("utf-8")

    actual = exs.serialize(tree, line_length=sys.maxsize, siblings=True)

    assert actual == expected
