// Binding-owned accounting only. Runtime media tests exercise native queues.
#include "../native/opus_carrier.h"

#include <cassert>
#include <limits>

using namespace pulsebeam::webrtc_sys::opus_carrier;

int main() {
  auto budget = std::make_shared<Budget>();
  const auto source = Register(budget);
  assert(source);
  const std::array<std::uint32_t, 2> recipients{11, 12};
  const auto first = Reserve(*budget, 6, recipients);
  const auto second = Reserve(*budget, 6, recipients);
  assert(first && second && budget->slots == 24);
  assert(!Reserve(*budget, 1, recipients));
  assert(!Reserve(*budget, 0, recipients));
  assert(!Reserve(*budget, 7, recipients));
  for (const auto packet : {*first, *second}) {
    for (std::uint8_t slot = 0; slot < 6; ++slot) {
      Acknowledge(source, packet, 11, slot);
      Acknowledge(source, packet, 11, slot);  // duplicate/reset generation
    }
  }
  assert(budget->slots == 12 && budget->pending.size() == 2);
  Acknowledge(source, *first, 99, 0);
  Acknowledge(source, *first, 12, 6);
  Acknowledge(source + 1, *first, 12, 0);
  assert(budget->slots == 12);
  const std::array<std::uint32_t, 2> duplicate{12, 12};
  const std::array<std::uint32_t, 1> invalid{0};
  assert(!Reserve(*budget, 1, duplicate));
  assert(!Reserve(*budget, 1, invalid));
  assert(budget->slots == 12);
  const auto third = Reserve(*budget, 6, recipients);
  assert(third && budget->slots == 24);
  // Consuming only a prefix must leave the tail reserved, even across reset.
  Acknowledge(source, *third, 11, 0);
  assert(budget->slots == 23);
  for (const auto packet : {*first, *second, *third}) {
    for (const auto recipient : recipients) {
      for (std::uint8_t slot = 0; slot < 6; ++slot)
        Acknowledge(source, packet, recipient, slot);
    }
  }
  assert(budget->slots == 0 && budget->pending.empty());
  // A detached/replaced sink generation cannot acknowledge a replacement.
  const std::array<std::uint32_t, 1> replacement{13};
  const auto fourth = Reserve(*budget, 2, replacement);
  assert(fourth && budget->slots == 2);
  Acknowledge(source, *fourth, 12, 0);
  assert(budget->slots == 2);
  Acknowledge(source, *fourth, 13, 0);
  assert(budget->slots == 1);
  // Closing removes lookup ownership, not a fictitious consumption receipt.
  Unregister(source);
  Acknowledge(source, *fourth, 13, 1);
  assert(budget->slots == 1);
  budget->next_packet = std::numeric_limits<std::uint32_t>::max();
  assert(!Reserve(*budget, 1, replacement));
  budget.reset();
  {
    auto fresh = std::make_shared<Budget>();
    const auto fresh_source = Register(fresh);
    assert(fresh_source != source);
    const auto packet = Reserve(*fresh, 1, replacement);
    assert(packet);
    Acknowledge(source, *packet, 13, 0);
    assert(fresh->slots == 1);
    Acknowledge(fresh_source, *packet, 13, 0);
    assert(fresh->slots == 0 && fresh->pending.empty());
    Unregister(fresh_source);
  }
}
