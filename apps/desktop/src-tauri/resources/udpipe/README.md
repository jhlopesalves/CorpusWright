# UDPipe Models

Bundled models are Universal Dependencies 2.1 models for UDPipe, sourced from
`https://github.com/bnosac/udpipe.models.ud`.

The bnosac repository publishes these model files as models whose licences allow
commercial use. Runtime loading verifies these SHA-256 checksums before a
bundled model is accepted:

| Language | File | Training source | Licence | SHA-256 |
| --- | --- | --- | --- | --- |
| English EWT | `english-ud-2.1-20180111.udpipe` | `UD_English`, renamed to `UD_English-EWT` in UD 2.2 | CC BY-SA 4.0 | `20432a6f87b1f258927207b8fbd2dc21ebff9722b381b7a36cef34c8c9a380dc` |
| French Sequoia | `french-sequoia-ud-2.1-20180111.udpipe` | `UD_French-Sequoia` | LGPL-LR | `d8f3ebb24b00a094b7fe23354730ca55900ad5de4cc07f710025aef949f454f4` |
| Portuguese Bosque | `portuguese-ud-2.1-20180111.udpipe` | `UD_Portuguese`, renamed to `UD_Portuguese-Bosque` in UD 2.2 | CC BY-SA 4.0 | `476c5d0d3f435449a4d42629259d0925dfdcd4da71f9175b88e892a6d09d4345` |

Do not replace these files with the LINDAT handle `11234/1-3131` UDPipe 2.5
models; those files are CC BY-NC-SA 4.0 and are not bundled here.
