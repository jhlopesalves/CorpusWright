# Third-Party Notices

CorpusWright itself is licensed under the MIT License (see `LICENSE`).
The following bundled third-party components are subject to their own
licences.

## PDFium native library

- **Component:** PDFium rendering engine (native library)
- **Source:** `github.com/bblanchon/pdfium-binaries` — prebuilt binaries
  from Google's [PDFium](https://pdfium.googlesource.com/pdfium/) project
- **Licence:** BSD-3-Clause
- **Windows DLL SHA-256:**
  `e472f04b57181946ca1e52fa34086079599d9b733164e7ab05179b03f4d3ddcc`
- **Location in repository:**
  `apps/desktop/src-tauri/resources/ocr/pdfium.dll`

## UDPipe Universal Dependencies 2.1 models

- **Component:** UDPipe Universal Dependencies 2.1 models
- **Source:** `https://github.com/bnosac/udpipe.models.ud`
- **Location in repository:** `apps/desktop/src-tauri/resources/udpipe/`
- **Source licence note:** bnosac states that the models in this repository
  are released under licences that allow commercial usage, and that each
  model's licence is specified in its corresponding `src/<treebank>/LICENSE`
  file.
- **Bundled models:**
  - `english-ud-2.1-20180111.udpipe`
    - Treebank: Universal Dependencies English Web Treebank
      (`UD_English`, later renamed to `UD_English-EWT`)
    - Model licence: Creative Commons Attribution-ShareAlike 4.0
      International (CC BY-SA 4.0)
    - SHA-256:
      `20432a6f87b1f258927207b8fbd2dc21ebff9722b381b7a36cef34c8c9a380dc`
  - `french-sequoia-ud-2.1-20180111.udpipe`
    - Treebank: Universal Dependencies French Sequoia
      (`UD_French-Sequoia`)
    - Model licence: Lesser General Public License For Linguistic Resources
      (LGPL-LR)
    - SHA-256:
      `d8f3ebb24b00a094b7fe23354730ca55900ad5de4cc07f710025aef949f454f4`
  - `portuguese-ud-2.1-20180111.udpipe`
    - Treebank: Universal Dependencies Portuguese Bosque
      (`UD_Portuguese`, later renamed to `UD_Portuguese-Bosque`)
    - Model licence: Creative Commons Attribution-ShareAlike 4.0
      International (CC BY-SA 4.0)
    - SHA-256:
      `476c5d0d3f435449a4d42629259d0925dfdcd4da71f9175b88e892a6d09d4345`
- **Model build attribution:** bnosac credits the `udpipe` R package, fastText
  word-vector work by Bojanowski, Grave, Joulin, and Mikolov, and the
  corresponding Universal Dependencies treebanks for the model builds.
