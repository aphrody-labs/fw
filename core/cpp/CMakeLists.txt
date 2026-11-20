cmake_minimum_required(VERSION 3.20)
project(fw_cachy_simd VERSION 4.2.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)

add_library(fw_cachy_simd STATIC fw_cachy_simd.cpp)
if(MSVC)
  target_compile_options(fw_cachy_simd PRIVATE /O2 /arch:AVX2)
else()
  target_compile_options(fw_cachy_simd PRIVATE -O3 -march=x86-64-v3 -flto=auto)
endif()
