// A plain C interface to the vendored ProgPoW implementation, for the Rust side.
// Hashes are 32 bytes in the byte order the library uses, which is how they are
// written in hex ("big-endian").

#include <crypto/ethash/include/ethash/ethash.hpp>
#include <crypto/ethash/include/ethash/progpow.hpp>

#include <cstdint>
#include <cstring>

namespace
{
ethash::hash256 load(const uint8_t* bytes) noexcept
{
    ethash::hash256 hash;
    std::memcpy(hash.bytes, bytes, 32);
    return hash;
}
}  // namespace

extern "C" {

// The light verification context of an epoch. Takes seconds to build for late epochs.
// Returns null when out of memory.
void* kawpow_context_new(int epoch) noexcept
{
    return ethash_create_epoch_context(epoch);
}

void kawpow_context_free(void* context) noexcept
{
    ethash_destroy_epoch_context(static_cast<ethash_epoch_context*>(context));
}

// The full proof of work: writes the final hash and the mix hash.
void kawpow_hash(const void* context, int height, const uint8_t* header_hash, uint64_t nonce,
    uint8_t* final_out, uint8_t* mix_out) noexcept
{
    const auto result = progpow::hash(
        *static_cast<const ethash_epoch_context*>(context), height, load(header_hash), nonce);
    std::memcpy(final_out, result.final_hash.bytes, 32);
    std::memcpy(mix_out, result.mix_hash.bytes, 32);
}

// The final hash that follows from a mix hash somebody else claims. Cheap, and it
// proves nothing about the mix hash itself.
void kawpow_final(int height, const uint8_t* header_hash, const uint8_t* mix_hash,
    uint64_t nonce, uint8_t* final_out) noexcept
{
    const auto result = progpow::hash_no_verify(height, load(header_hash), load(mix_hash), nonce);
    std::memcpy(final_out, result.bytes, 32);
}

void kawpow_seed(int epoch, uint8_t* seed_out) noexcept
{
    const auto seed = ethash_calculate_epoch_seed(epoch);
    std::memcpy(seed_out, seed.bytes, 32);
}

}  // extern "C"
