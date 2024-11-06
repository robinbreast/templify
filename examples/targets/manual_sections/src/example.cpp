#include "example.hpp"
#include <iostream>

namespace example
{
  Example::Example()
  {
    std::cout << "Example::Example()" << std::endl;
    // MANUAL SECTION START: init-custom-variables
    custom_variable = 1;
    // MANUAL SECTION END
  }

  Example::~Example()
  {
    std::cout << "Example::~Example()" << std::endl;
  }

  void Example::print()
  {
    std::cout << "Example::print()" << std::endl;
  }
} // namespace example
