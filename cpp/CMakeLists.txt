cmake_minimum_required(VERSION 3.20)
# The version is product.toml's `version` (the numeric part): one source for the whole product.
set(FW_PRODUCT_TOML "${CMAKE_CURRENT_LIST_DIR}/../product.toml")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${FW_PRODUCT_TOML}")
file(STRINGS "${FW_PRODUCT_TOML}" FW_PRODUCT_VERSION_LINE REGEX "^version = \"" LIMIT_COUNT 1)
string(REGEX REPLACE "^version = \"([0-9]+(\\.[0-9]+)*).*$" "\\1" FW_PRODUCT_VERSION "${FW_PRODUCT_VERSION_LINE}")
project(fw_cachy_simd VERSION ${FW_PRODUCT_VERSION} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)

add_library(fw_cachy_simd STATIC fw_cachy_simd.cpp)
if(MSVC)
  target_compile_options(fw_cachy_simd PRIVATE /O2 /arch:AVX2)
else()
  target_compile_options(fw_cachy_simd PRIVATE -O3 -march=x86-64-v3 -flto=auto)
endif()
