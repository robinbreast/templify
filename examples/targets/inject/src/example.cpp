#include "example.hpp"
// missing include: <iostream>

namespace example
{
  Example::Example()
  {
    std::cout << "Example::Example()" << std::endl;
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
