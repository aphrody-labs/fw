#include "cpp/edge_abi/edge_abi.h"

#include <array>
#include <cstdint>
#include <vector>

#include "gtest/gtest.h"

namespace ay_cf {
namespace {

TEST(EdgeAbiTest, RoundTripAndCrc32cVerification) {
  const std::vector<uint8_t> payload = {'r', '2', '-', 'a', 's', 's', 'e', 't', '-', 'o', 'k'};
  EdgeFrameHeader hdr{
      .version = kEdgeFrameVersion,
      .flags = 0x0001,
      .payload_bytes = payload.size(),
      .payload_crc32c = ComputeR2Crc32c(payload),
      .stream_id = 42,
  };

  const auto wire = EncodeFrameHeader(hdr);
  const auto decoded = DecodeFrameHeader(wire);
  ASSERT_TRUE(decoded.ok()) << decoded.status();
  EXPECT_EQ(decoded->version, kEdgeFrameVersion);
  EXPECT_EQ(decoded->flags, 0x0001);
  EXPECT_EQ(decoded->payload_bytes, payload.size());
  EXPECT_EQ(decoded->stream_id, 42u);

  EXPECT_TRUE(VerifyFrame(wire, payload).ok());

  std::vector<uint8_t> tampered = payload;
  tampered[0] ^= 0xFF;
  EXPECT_FALSE(VerifyFrame(wire, tampered).ok());
}

TEST(EdgeAbiTest, RejectsInvalidMagicOrTruncatedHeader) {
  const std::array<uint8_t, 8> short_buf{};
  EXPECT_FALSE(DecodeFrameHeader(short_buf).ok());

  std::array<uint8_t, kEdgeFrameHeaderBytes> bad_magic{};
  bad_magic[0] = 'X';
  EXPECT_FALSE(DecodeFrameHeader(bad_magic).ok());
}

}  // namespace
}  // namespace ay_cf
