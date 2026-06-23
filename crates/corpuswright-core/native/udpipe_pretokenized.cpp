#include "udpipe.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

using ufal::udpipe::model;
using ufal::udpipe::sentence;

namespace {

std::string& last_error() {
  static thread_local std::string error;
  return error;
}

struct CwUdpipeModel {
  std::unique_ptr<model> m;
};

struct CwUdpipeTaggedResult {
  std::vector<std::string> forms;
  std::vector<std::string> lemmas;
  std::vector<std::string> upostags;
  std::vector<std::string> xpostags;
  std::vector<std::string> feats;
  std::vector<std::string> miscs;
  std::vector<int32_t> ids;
  int32_t range_count = 0;
};

}  // namespace

extern "C" {

struct CwUdpipeWord {
  const char* form;
  const char* lemma;
  const char* upostag;
  const char* xpostag;
  const char* feats;
  const char* misc;
  int32_t id;
};

CwUdpipeModel* cw_udpipe_model_load(const char* model_path) {
  last_error().clear();
  if (model_path == nullptr) {
    last_error() = "Invalid null model path";
    return nullptr;
  }

  std::unique_ptr<model> loaded_model(model::load(model_path));
  if (!loaded_model) {
    last_error() = "Failed to load UDPipe model from: ";
    last_error() += model_path;
    return nullptr;
  }

  auto* wrapper = new CwUdpipeModel();
  wrapper->m = std::move(loaded_model);
  return wrapper;
}

void cw_udpipe_model_free(CwUdpipeModel* model) {
  delete model;
}

CwUdpipeTaggedResult* cw_udpipe_tag_pretokenized(
    CwUdpipeModel* model,
    const char* const* tokens,
    size_t token_count) {
  last_error().clear();
  if (model == nullptr || !model->m) {
    last_error() = "Invalid null UDPipe model";
    return nullptr;
  }
  if (tokens == nullptr && token_count > 0) {
    last_error() = "Invalid null token array";
    return nullptr;
  }

  sentence tagged_sentence;
  for (size_t idx = 0; idx < token_count; idx++) {
    if (tokens[idx] == nullptr) {
      last_error() = "Invalid null token surface";
      return nullptr;
    }
    tagged_sentence.add_word(tokens[idx]);
  }

  std::string error;
  if (!model->m->tag(tagged_sentence, model::DEFAULT, error)) {
    last_error() = error.empty() ? "UDPipe tagger returned false" : error;
    return nullptr;
  }

  auto* result = new CwUdpipeTaggedResult();
  result->range_count =
      static_cast<int32_t>(tagged_sentence.multiword_tokens.size());
  for (size_t idx = 1; idx < tagged_sentence.words.size(); idx++) {
    const auto& word = tagged_sentence.words[idx];
    result->forms.push_back(word.form);
    result->lemmas.push_back(word.lemma);
    result->upostags.push_back(word.upostag);
    result->xpostags.push_back(word.xpostag);
    result->feats.push_back(word.feats);
    result->miscs.push_back(word.misc);
    result->ids.push_back(static_cast<int32_t>(word.id));
  }

  return result;
}

void cw_udpipe_tagged_result_free(CwUdpipeTaggedResult* result) {
  delete result;
}

int32_t cw_udpipe_result_word_count(CwUdpipeTaggedResult* result) {
  if (result == nullptr) {
    return 0;
  }
  return static_cast<int32_t>(result->forms.size());
}

int32_t cw_udpipe_result_range_count(CwUdpipeTaggedResult* result) {
  if (result == nullptr) {
    return 0;
  }
  return result->range_count;
}

CwUdpipeWord cw_udpipe_result_get_word(
    CwUdpipeTaggedResult* result,
    int32_t index) {
  CwUdpipeWord word = {
      nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, 0};

  if (result == nullptr || index < 0 ||
      static_cast<size_t>(index) >= result->forms.size()) {
    return word;
  }

  const auto idx = static_cast<size_t>(index);
  word.form = result->forms[idx].c_str();
  word.lemma = result->lemmas[idx].c_str();
  word.upostag = result->upostags[idx].c_str();
  word.xpostag = result->xpostags[idx].c_str();
  word.feats = result->feats[idx].c_str();
  word.misc = result->miscs[idx].c_str();
  word.id = result->ids[idx];
  return word;
}

const char* cw_udpipe_get_error() {
  return last_error().empty() ? nullptr : last_error().c_str();
}

}  // extern "C"
