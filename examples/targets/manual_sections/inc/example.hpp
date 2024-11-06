#pragma once

namespace example
{
  class Example {
  public:
    Example();
    ~Example();

    void print();
  private:
    // MANUAL SECTION START: custom-variables
    int custom_variable{0};
    // MANUAL SECTION END
  };
} // namespace example
