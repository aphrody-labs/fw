#include <iostream>
#include <vector>
#include <numeric>
#include "../c/fw_abi.h"

int main() {
    std::vector<float> a = {1.0f, 2.0f, 3.0f, 4.0f};
    std::vector<float> b = {5.0f, 6.0f, 7.0f, 8.0f};
    float dot = std::inner_product(a.begin(), a.end(), b.begin(), 0.0f);
    std::cout << "{\"ok\":true,\"lang\":\"cpp\",\"dot\":" << dot << "}\n";
    return (dot > 69.9f && dot < 70.1f) ? 0 : 1;
}
