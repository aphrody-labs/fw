#pragma once

#include <array>
#include <cstdint>

#include "absl/status/statusor.h"
#include "absl/types/span.h"

namespace ay_cf {

inline constexpr std::array<uint8_t, 4> kEdgeFrameMagic = {'A', 'Y', 'C', 'F'};
inline constexpr uint16_t kEdgeFrameVersion = 1;
inline constexpr size_t kEdgeFrameHeaderBytes = 24;

// Binary header aligned with Cloudflare R2 S3 x-amz-checksum-crc32c (Abseil CRC32C).
struct EdgeFrameHeader {
  uint16_t version = kEdgeFrameVersion;
  uint16_t flags = 0;
  uint64_t payload_bytes = 0;
  uint32_t payload_crc32c = 0;
  uint32_t stream_id = 0;
};

// Computes hardware-accelerated R2/S3 CRC32C checksum over a byte payload.
uint32_t ComputeR2Crc32c(absl::Span<const uint8_t> payload);

// Serializes an EdgeFrameHeader into a 24-byte big-endian wire buffer.
std::array<uint8_t, kEdgeFrameHeaderBytes> EncodeFrameHeader(const EdgeFrameHeader& header);

// Parses and validates a 24-byte wire buffer using absl::StatusOr.
absl::StatusOr<EdgeFrameHeader> DecodeFrameHeader(absl::Span<const uint8_t> wire);

// Validates both frame header and payload CRC32C integrity.
absl::Status VerifyFrame(absl::Span<const uint8_t> wire_header,
                         absl::Span<const uint8_t> payload);

}  // namespace ay_cf
